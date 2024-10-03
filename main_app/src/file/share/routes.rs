use crate::auth::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, FileType};
use crate::libs::{check_file_permission, mongo_error_check, ApiError};
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::Collection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use rocket::serde::json::Json;

use super::super::storage_backend::lib::StorageFactory;
use std::sync::Arc;
use rocket::tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct CrateShareLinkRequest<'r> {
    target_uuid: &'r str,
    live_second: i64,
    download_count_limit: i64,//-1 for no limit
    password: Option<&'r str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrateShareLinkResponse {
    status: String,
    link: String,
    info: String,
}

#[post("/crate", data = "<request>")]
pub async  fn crate_share_link(
    request: Json<CrateShareLinkRequest<'_>>,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
) -> Result<Json<CrateShareLinkResponse>, ApiError> {

    let db = &mongo.database;
    let collection = db.collection::<File>("files");
    
    let metadata = collection.find_one(doc! { "_id": ObjectId::from_str(request.target_uuid).unwrap() }).await;
    let metadata = mongo_error_check(metadata, Some("File"))?;

    check_file_permission(&user, &metadata)?;

    let share_link_uuid = uuid::Uuid::new_v4().to_string();

    let _: () = redis.set(&share_link_uuid, &metadata._id.to_string()).await;
    let _: () = redis.expire(&share_link_uuid, request.live_second).await;
    let _: () = redis.set(format!("{}_limit", share_link_uuid), &request.download_count_limit).await;
    let _: () = redis.expire(format!("{}_limit", share_link_uuid), request.live_second).await;

    if request.password.is_some() {
        let _: () = redis.set(format!("{}_password", share_link_uuid).as_str(), request.password.unwrap()).await;
        let _: () = redis.expire(format!("{}_password", share_link_uuid).as_str(), request.live_second).await;
    }

    Ok(Json(CrateShareLinkResponse {
        status: "Success".to_string(),
        link: share_link_uuid,
        info: "".to_string(),
    }))
}

use super::super::lib::CustomFileResponse;

#[derive(Responder)]
pub enum GetFileResponse {
    #[response(status = 200)]
    File(CustomFileResponse),
    #[response(status = 200)]
    Metadata(Json<File>),
}

async fn path_find(mut path: Vec<&str>, file_metadata: File, collation: Collection<File>) -> Result<File, ApiError> {
    if path.is_empty() {
        return Ok(file_metadata);
    }
    let t = path.pop().unwrap();
    if !file_metadata.children.contains(&ObjectId::from_str(t).unwrap()) {
        return Err(ApiError::NotFound("File not found".to_string().into()));
    }

    let file_metadata = collation.find_one(doc! { "_id": ObjectId::from_str(t).unwrap() }).await;
    let file_metadata = mongo_error_check(file_metadata, Some("File"))?;

    Box::pin(async move {
        path_find(path, file_metadata, collation).await
    }).await
}

#[get("/<uuid>?<path>&<password>&<metadata>")]
pub async fn get_share_file(
    uuid: &str,
    path: Option<&str>,
    password: Option<&str>,
    metadata: Option<bool>,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    storage_factory: &rocket::State<Arc<Mutex<StorageFactory>>>,
) -> Result<GetFileResponse, ApiError> {

    let db = &mongo.database;
    let collection = db.collection::<File>("files");
    if !redis.exists(&uuid).await {
        return Err(ApiError::NotFound("Link not found or expired".to_string().into()));
    }
    if redis.exists(format!("{}_password", uuid).as_str()).await {
        let true_password: String = redis.get(format!("{}_password", uuid).as_str()).await;

        if true_password.is_empty() || password.is_none() {
            return Err(ApiError::BadRequest("Wrong password".to_string().into()));
        }

        if true_password != password.unwrap() {
            return Err(ApiError::BadRequest("Wrong password".to_string().into()));
        }
    }

    let limit: i64 = redis.get(format!("{}_limit", uuid).as_str()).await;
    if limit == 0 {
        return Err(ApiError::BadRequest("Download limit reached".to_string().into()));
    }
    let file_id: String = redis.get(&uuid).await;
    let file_metadata = collection.find_one(doc! { "_id": ObjectId::from_str(file_id.as_str()).unwrap() }).await;

    let file_metadata = mongo_error_check(file_metadata, Some("File"))?;

    let metadata = metadata.unwrap_or(false);

    if path.is_none() {
        if metadata {
            return Ok(GetFileResponse::Metadata(Json(file_metadata)));
        }
        match file_metadata.type_ {
            FileType::File => {
                let _ = redis.decr(format!("{}_limit", uuid).as_str()).await;
                return Ok(GetFileResponse::File(CustomFileResponse::new(file_metadata, storage_factory,mongo).await?));
            },
            _ => {
                return Err(ApiError::BadRequest("Target is not a file".to_string().into()));
            }
        }
    }

    let path = path.unwrap();
    let mut path = path.split("/").collect::<Vec<&str>>();
    path.reverse();

    let file_metadata = path_find(path, file_metadata, collection).await?;

    if metadata {
        return Ok(GetFileResponse::Metadata(Json(file_metadata)));
    }

    match file_metadata.type_ {
        FileType::File => {
            let _ = redis.decr(format!("{}_limit", uuid).as_str()).await;
            Ok(GetFileResponse::File(CustomFileResponse::new(file_metadata, storage_factory,mongo).await?))
        },
        _ => {
            Err(ApiError::BadRequest("Target is not a file".to_string().into()))
        }
    }
}

