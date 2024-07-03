use crate::auth::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, FileType};
use crate::public::{check_file_permission, generate_file_path, mongo_error_check, ApiError};
use crate::MyConfig;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use rocket::data::{Data, ToByteUnit};
use rocket::response::{self, status};
use rocket::response::Response;
use rocket::response::Responder;
use rocket::Request;
use rocket::http::Header;
use std::fs;
use std::str::FromStr;
use rocket::tokio::fs::File as AsyncFile;
use super::lib::{check_file_sha256, file_sha256};

pub struct CustomFileResponse {
    response: Response<'static>,
}

impl<'r> Responder<'r, 'static> for CustomFileResponse {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        Ok(self.response)
    }
}

#[get("/<uuid>")]
pub async fn get_file(
    uuid: &str,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    config: &rocket::State<MyConfig>,
) -> Result<CustomFileResponse, ApiError> {
    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let file = collection.find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() }, None).await;

    let file = mongo_error_check(file, Some("File"))?;

    let _ = check_file_permission(&user, &file)?;

    match file.type_ {
        FileType::File => {
            let file_path = generate_file_path(&file, &config);

            let ext = rocket::http::ContentType::from_extension(file.name.split('.').last().unwrap());

            let file_ = AsyncFile::open(file_path).await.unwrap();
            let mut response = Response::build();
            response
                .header(if let Some(ext) = ext { ext } else { rocket::http::ContentType::Binary })
                .header(Header::new("Content-Disposition", format!("attachment; filename=\"{}\"", file.name)))
                .streamed_body(file_);

            Ok(CustomFileResponse { response: response.finalize() })

        }
        _ => {
            return Err(ApiError::NotFound("Target is not a file".to_string().into()))
        }
    }
}

#[post("/<uuid>", data = "<file>")]
pub async fn upload_file(
    uuid: &str,
    user: AuthenticatedUser,
    file: Data<'_>,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    config: &rocket::State<MyConfig>,
) -> Result<status::NoContent, ApiError> {

    if !redis.exists(uuid).await {
        return Err(ApiError::NotFound("Metadata not found".to_string().into()));
    };
    let metadata: File = serde_json::from_str(redis.get(&uuid).await.unwrap().as_str()).unwrap();
    let _ = check_file_permission(&user, &metadata)?;

    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let file_path = generate_file_path(&metadata, &config);

    match file.open(128.kibibytes()).into_file(&file_path).await {
        Ok(_) => {},
        Err(_) => return Err(ApiError::InternalServerError(None)),
    }

    match check_file_sha256(&file_path, &metadata.sha256).await {
        Ok(_) => {},
        Err(_) => {
            let _ = fs::remove_file(file_path.clone());
            return Err(ApiError::BadRequest("Hash not match".to_string().into()));
        }
    }

    let metadata = File {
        size: fs::metadata(file_path).unwrap().len(),
        ..metadata
    };
    let _ = collection.insert_one(metadata.clone(), None).await;
    let father = collection
        .find_one(doc! { "_id": metadata.father }, None)
        .await
        .unwrap()
        .unwrap();
    let mut t = father.children;
    t.push(metadata._id);
    let _ = collection
        .update_one(
            doc! { "_id": father._id },
            doc! { "$set": { "children": t } },
            None,
        )
        .await;
    let _ = redis.delete(uuid).await;

    Ok(status::NoContent)
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
    config: &rocket::State<MyConfig>,
) -> Result<status::NoContent, ApiError> {
    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");
    let metadata = match collection.find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() }, None).await {
        Ok(metadata) => metadata,
        Err(_) => return Err(ApiError::InternalServerError(None)),
    };
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => return Err(ApiError::NotFound("File not found".to_string().into())),
    };
    if metadata.owner != user.uuid {
        return Err(ApiError::Forbidden("Permission denied".to_string().into()));
    }
    match metadata.type_ {
        FileType::File => {}
        _ => {
            return Err(ApiError::BadRequest("Target is not a file".to_string().into()));
        }
    }

    let file_path = if metadata.path == "FLAT" {
        format!("{}/flat/{}", config.storage_path, metadata._id)
    } else {
        format!("{}/{}", config.storage_path, metadata.path)
    };

    let _ = fs::rename(file_path.clone(), file_path.clone() + ".tmp");
    let _ = form.file.persist_to(file_path.clone()).await.unwrap();

    let hash = file_sha256(file_path.clone().as_str()).await;

    if hash != form.sha256 {
        let _ = fs::rename(file_path.clone() + ".tmp", file_path);
        return Err(ApiError::BadRequest("Hash not match".to_string().into()));
    }

    let _ = collection
        .update_one(
            doc! { "_id": metadata._id },
            doc! { "$set": doc! { "sha256": hash },
                    "$set": doc! { "updated_at": chrono::Utc::now().timestamp() },
                    "$set": doc! { "size": fs::metadata(file_path).unwrap().len() as i64 },
            },
            None,
        )
        .await;
    Ok(status::NoContent)
}

//删除在metadata那里，不提供直接删除文件的功能
