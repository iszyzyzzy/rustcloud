use std::str::FromStr;

use crate::auth::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, FileType};
use crate::file::storage_backend::lib::StorageFactory;
use crate::libs::{mongo_error_check, ApiError};
use chrono::Utc;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, Serialize, Deserialize)]
pub struct MetaDataCreateRequest {
    pub name: String,
    pub type_: FileType,
    pub father: String,
    pub size: u64,
    pub sha256: String,
    pub storage_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaDataCreateResponse {
    pub id: String,
    pub status: String, //"success" / "ref" //ref表示服务端找到了sha256一样的文件，无须上传
}

impl MetaDataCreateResponse {
    fn normal(id: String) -> Self {
        MetaDataCreateResponse {
            id,
            status: "success".to_string(),
        }
    }
    fn ref_file(id: String) -> Self {
        MetaDataCreateResponse {
            id,
            status: "ref".to_string(),
        }
    }
}

//收到metadata的时候先存到redis里，等到收到文件再写入mongodb
//文件夹就直接写入
#[post("/create", data = "<metadata>")]
pub async fn add_metadata(
    metadata: Json<MetaDataCreateRequest>,
    user: AuthenticatedUser,
    redis: &rocket::State<Redis>,
    mongo: &rocket::State<MongoDb>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<Json<MetaDataCreateResponse>, ApiError> {
    let id = ObjectId::new();
    let metadata = metadata.into_inner();
    let factory = storage_factory.lock().await;
    match factory.get_backend(&metadata.storage_type) {
        Some(_) => {}
        None => {
            return Err(ApiError::BadRequest(
                "Storage backend not found".to_string().into(),
            ))
        }
    }
    let metadata = File {
        _id: id,
        name: metadata.name,
        type_: metadata.type_,
        father: ObjectId::from_str(&metadata.father).unwrap(),
        size: metadata.size,
        sha256: metadata.sha256,
        owner: user.uuid,
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        children: vec![],
        path: "FLAT".to_string(),
        storage_type: metadata.storage_type,
    };
    let father = mongo
        .database
        .collection::<File>("files")
        .find_one(doc! { "_id": metadata.father })
        .await;
    match father {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(ApiError::NotFound(
                "Father folder not found".to_string().into(),
            ))
        }
        Err(_) => {
            return Err(ApiError::InternalServerError(
                "Database error".to_string().into(),
            ))
        }
    }
    match metadata.type_ {
        FileType::Folder => {
            let db = mongo.database.collection::<File>("files");
            let _ = db.insert_one(metadata.clone()).await;
            let father = db
                .find_one(doc! { "_id": metadata.father })
                .await
                .unwrap()
                .unwrap();
            let mut t = father.children;
            t.push(metadata._id);
            let _ = db
                .update_one(
                    doc! { "_id": father._id },
                    doc! { "$set": { "children": t } },
                )
                .await;
            Ok(Json(MetaDataCreateResponse::normal(id.to_string())))
        }
        FileType::File => {
            let db = mongo.database.collection::<File>("files");
            match db.find_one(doc! { "sha256": &metadata.sha256 , "storage_type": doc! {"$ne": "ref"}}).await {
                Ok(Some(existed)) => {
                    let metadata = File {
                        storage_type: "ref".to_string(),
                        path: existed._id.to_hex(),
                        ..metadata
                    };
                    let _ = db.insert_one(metadata.clone()).await;
                    return Ok(Json(MetaDataCreateResponse::ref_file(id.to_string())));
                }
                _ => {
                    let _: () = redis
                        .set(
                            id.to_string().as_str(),
                            serde_json::to_string(&metadata).unwrap().as_str(),
                        )
                        .await;
                    let _: () = redis.expire(id.to_string().as_str(), 24 * 60 * 60).await;
                    Ok(Json(MetaDataCreateResponse::normal(id.to_string())))
                }
            }
        }
        _ => Err(ApiError::Forbidden("Permission denied".to_string().into())),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileTree {
    pub _id: ObjectId,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: FileType,
    pub father: ObjectId,
    pub children: Vec<FileTree>,
    pub owner: ObjectId,
    pub created_at: i64,
    pub updated_at: i64,
    pub size: u64,
    pub sha256: String,
    pub path: String,
}

async fn get_tree(
    root_id: &ObjectId,
    mongo: &MongoDb,
) -> Result<FileTree, Box<dyn std::error::Error + Send + Sync>> {
    let db = mongo.database.collection::<File>("files");
    let file = db.find_one(doc! {"_id": root_id}).await.unwrap();
    match file {
        Some(file) => {
            let mut children = vec![];
            for child_id in file.children {
                children.push(Box::pin(get_tree(&child_id, mongo)).await?);
            }
            let tree = FileTree {
                _id: file._id,
                name: file.name,
                type_: file.type_,
                father: file.father,
                children,
                owner: file.owner,
                created_at: file.created_at,
                updated_at: file.updated_at,
                size: file.size,
                sha256: file.sha256,
                path: file.path,
            };
            Ok(tree)
        }
        None => Err("File not found".into()),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    File(File),
    FileTree(FileTree),
}

fn check_permission(user: AuthenticatedUser, file: &File) -> Result<(), ApiError> {
    if file.owner != user.uuid {
        return Err(ApiError::Forbidden("Permission denied".to_string().into()));
    };
    Ok(())
}

#[get("/<uuid>?<tree>")]
pub async fn get_metadata(
    uuid: &str,
    tree: Option<bool>,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
) -> Result<Json<Response>, ApiError> {
    let tree = tree.unwrap_or(false);
    let db = mongo.database.collection::<File>("files");
    let file = db
        .find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()})
        .await;
    let file = mongo_error_check(file, Some("File"))?;
    check_permission(user, &file)?;
    if tree {
        let tree = get_tree(&ObjectId::from_str(uuid).unwrap(), mongo).await;
        match tree {
            Ok(tree) => {
                return Ok(Json(Response::FileTree(tree)));
            }
            Err(_) => {
                return Err(ApiError::InternalServerError(
                    "Unknown error during tree generation".to_string().into(),
                ))
            }
        }
    }
    Ok(Json(Response::File(file)))
}

#[put("/<uuid>", data = "<metadata>")]
pub async fn update_metadata(
    uuid: &str,
    metadata: Json<File>,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
) -> Result<status::NoContent, ApiError> {
    let new_metadata = metadata.into_inner();
    let db = mongo.database.collection::<File>("files");
    let file = db
        .find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()})
        .await;
    let file = mongo_error_check(file, Some("File"))?;
    //其实只有father,name可以更新
    //sha256和size是在文件更新的时候改的
    //path,type不能更新
    //created_at和updated_at是由系统指定的
    //owner肯定不能动
    //查一下
    if new_metadata.sha256 != file.sha256
        || new_metadata.size != file.size
        || new_metadata.type_ != file.type_
        || new_metadata.path != file.path
        || new_metadata.owner != file.owner
        || new_metadata.created_at != file.created_at
    {
        return Err(ApiError::BadRequest(
            "Only father and name can be updated".to_string().into(),
        ));
    };
    //再直接覆写掉updated_at
    let new_metadata = File {
        updated_at: Utc::now().timestamp(),
        ..new_metadata
    };
    check_permission(user, &file)?;
    let old_father = match db.find_one(doc! {"_id": file.father}).await {
        Ok(Some(file)) => file,
        Ok(None) => {
            return Err(ApiError::InternalServerError(
                "Unexpected error".to_string().into(),
            ))
        } //这是不应该发生的情况
        Err(_) => {
            return Err(ApiError::InternalServerError(
                "Database error".to_string().into(),
            ))
        }
    };
    let new_father = match db.find_one(doc! {"_id": new_metadata.father}).await {
        Ok(Some(file)) => file,
        Ok(None) => {
            return Err(ApiError::NotFound(
                "New father folder not found".to_string().into(),
            ))
        }
        Err(_) => {
            return Err(ApiError::InternalServerError(
                "Database error".to_string().into(),
            ))
        }
    };
    let mut old_father_children = old_father.children;
    let mut new_father_children = new_father.children;
    old_father_children.retain(|x| *x != file._id);
    new_father_children.push(file._id);
    let _ = db
        .update_one(
            doc! {"_id": old_father._id},
            doc! { "$set": doc! { "children": old_father_children },
            "$set": doc! { "updated_at": Utc::now().timestamp() } },
        )
        .await;
    let _ = db
        .update_one(
            doc! {"_id": new_father._id},
            doc! { "$set": doc! { "children": new_father_children },
            "$set": doc! { "updated_at": Utc::now().timestamp() } },
        )
        .await;
    let _ = db
        .replace_one(
            doc! {"_id": ObjectId::from_str(uuid).unwrap()},
            new_metadata,
        )
        .await;
    Ok(status::NoContent)
}

#[delete("/<uuid>")]
pub async fn delete_metadata(
    uuid: &str,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<status::NoContent, ApiError> {
    if redis.exists(uuid).await {
        let _: () = redis.delete(uuid).await;
        return Ok(status::NoContent);
    }
    let db = mongo.database.collection::<File>("files");
    let file = db
        .find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()})
        .await;
    let file = mongo_error_check(file, Some("File"))?;
    check_permission(user, &file)?;
    let father = db.find_one(doc! {"_id": file.father}).await;
    let father = mongo_error_check(father, Some("File"))?;
    let mut father_children = father.children;
    father_children.retain(|x| x != &file._id);
    let _ = db
        .update_one(
            doc! { "_id": father._id },
            doc! { "$set": { "children": father_children } },
        )
        .await
        .unwrap();
    let _ = db
        .delete_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()})
        .await
        .unwrap();
    let factory = storage_factory.lock().await;
    let _ = factory.delete_file(&file).await;
    Ok(status::NoContent)
}
