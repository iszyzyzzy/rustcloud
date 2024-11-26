use crate::auth::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, FileType};
use crate::libs::{check_file_permission, mongo_error_check, ApiError};
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use rocket::response::status;
use std::str::FromStr;

use super::lib::CustomFileResponse;
use super::storage_backend::lib::StorageFactory;
use super::storage_backend::ref_storage;
use rocket::tokio::sync::Mutex;
use std::sync::Arc;

#[get("/<uuid>")]
pub async fn get_file(
    uuid: &str,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<CustomFileResponse, ApiError> {
    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let metadata = collection
        .find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() })
        .await;

    let metadata = mongo_error_check(metadata, Some("File"))?;

    check_file_permission(&user, &metadata)?;

    match metadata.type_ {
        FileType::File => Ok(CustomFileResponse::new(metadata, storage_factory, mongo).await?),
        _ => Err(ApiError::NotFound(
            "Target is not a file".to_string().into(),
        )),
    }
}

#[post("/<uuid>", data = "<file>")]
pub async fn upload_file(
    uuid: &str,
    user: AuthenticatedUser,
    mut file: TempFile<'_>,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<status::NoContent, ApiError> {
    if !redis.exists(uuid).await {
        return Err(ApiError::NotFound("Metadata not found".to_string().into()));
    };
    let metadata: String = redis.get(uuid).await;
    let metadata: File = serde_json::from_str(metadata.as_str()).unwrap();
    check_file_permission(&user, &metadata)?;

    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let factory = storage_factory.lock().await;

    let file_type = super::lib::get_file_type(&file).await;
    let save_result = factory
        .check_sha256_and_save(&metadata, None, &mut file)
        .await?;

    let metadata = File {
        size: save_result.size,
        ..metadata
    };
    let _ = collection.insert_one(metadata.clone()).await;
    let _ = collection
        .update_one(
            doc! { "_id": metadata.father },
            doc! { "$push": { "children": metadata._id } },
        )
        .await;
    let _: () = redis.delete(uuid).await;
    match file_type {
        _ => Ok(status::NoContent),
    }
}

use rocket::form::Form;
use rocket::fs::TempFile;

#[derive(FromForm)]
pub struct UpdateFileRequest<'r> {
    pub sha256: String,
    pub file: TempFile<'r>,
}

#[put("/<uuid>", data = "<form>")]
pub async fn update_file(
    uuid: &str,
    user: AuthenticatedUser,
    mut form: Form<UpdateFileRequest<'_>>,
    mongo: &rocket::State<MongoDb>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<status::NoContent, ApiError> {
    let db = &mongo.database;
    let collection = db.collection::<File>("files");
    let metadata = mongo_error_check(
        collection
            .find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() })
            .await,
        Some("File"),
    )?;
    check_file_permission(&user, &metadata)?;
    match metadata.type_ {
        FileType::File => {}
        _ => {
            return Err(ApiError::BadRequest(
                "Target is not a file".to_string().into(),
            ));
        }
    }
    //先看看原本是不是ref,是的话先清理原本的ref
    if metadata.storage_type.as_str() == "ref" {
        ref_storage::remove_ref(&collection, &metadata).await?;
    }
    //如果是ref_mother的话要change_mother
    if let Some(ext) = &metadata.extra_metadata {
        if ext.file_references.len() > 0 {
            ref_storage::change_mother(&collection, &metadata).await?;
        }
    }
    //看看新的是否可以ref
    //遇到可以ref的就直接ref然后返回，不管传上来的是什么了
    if let Some(ref_mother) = ref_storage::find_and_add_ref(&collection, &metadata).await?
    {
        let _ = collection
            .update_one(
                doc! { "_id": metadata._id },
                doc! { "$set": doc! { "storage_type": "ref" },
                            "$set": doc! { "path": ref_mother._id.to_hex() },
                            "$set": doc! { "updated_at": chrono::Utc::now().timestamp() },
                            "$set": doc! { "sha256": ref_mother.sha256.clone() },
                            "$set": doc! { "size": ref_mother.size as i64 },
                },
            )
            .await;
        return Ok(status::NoContent);
    }

    let factory = storage_factory.lock().await;
    let save_result = factory
        .check_sha256_and_save(&metadata, Some(&form.sha256.clone()), &mut form.file)
        .await?;

    let _ = collection
        .update_one(
            doc! { "_id": metadata._id },
            doc! { "$set": doc! { "sha256": save_result.sha256 },
                    "$set": doc! { "updated_at": chrono::Utc::now().timestamp() },
                    "$set": doc! { "size": save_result.size as i64 },
            },
        )
        .await;
    Ok(status::NoContent)
}

//删除在metadata那里，不提供直接删除文件的功能
