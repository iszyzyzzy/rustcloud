use crate::auth::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, FileType};
use crate::public::{ApiError, CustomResponse};
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
) -> Result<CustomFileResponse, CustomResponse> {
    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let file = match collection.find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() }, None).await {
        Ok(file) => file,
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    };

    let file = match file {
        Some(file) => file,
        None => return Err(ApiError::NotFound("File not found".to_string().into()).to_response()),
    };

    if file.owner != user.uuid {
        return Err(ApiError::Forbidden("Permission denied".to_string().into()).to_response());
    }

    match file.type_ {
        FileType::File => {
            let file_path = if file.path == "FLAT" {
                format!("{}/flat/{}", config.storage_path, file._id)
            } else {
                format!("{}/{}", config.storage_path, file.path)
            };

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
            return Err(ApiError::NotFound("Target is not a file".to_string().into()).to_response())
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
) -> Result<status::NoContent, CustomResponse> {
    if !redis.exists(uuid).await {
        return Err(ApiError::NotFound("Metadata not found".to_string().into()).to_response());
    };
    let metadata: File = serde_json::from_str(redis.get(&uuid).await.unwrap().as_str()).unwrap();
    if metadata.owner != user.uuid {
        return Err(ApiError::Forbidden("Permission denied".to_string().into()).to_response());
    }
    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");

    let file_path = if metadata.path == "FLAT" {
        format!("{}/flat/{}", config.storage_path, metadata._id)
    } else {
        format!("{}/{}", config.storage_path, metadata.path)
    };

    match file.open(128.kibibytes()).into_file(&file_path).await {
        Ok(_) => {}
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    }

    let hash = file_sha256(file_path.clone().as_str()).await;
    if hash != metadata.sha256 {
        let _ = fs::remove_file(file_path.clone());
        return Err(ApiError::BadRequest("Hash not match".to_string().into()).to_response());
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

use rocket::tokio::io::{AsyncReadExt, BufReader};
use sha2::{Digest, Sha256};

pub async fn file_sha256(file_path: &str) -> String {
    let file = AsyncFile::open(&file_path).await.unwrap();

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];
    let mut reader = BufReader::new(file);

    loop {
        let n = reader.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let hash = hasher.finalize();
    let hash = format!("{:x}", hash);

    hash
}

#[put("/<uuid>", data = "<form>")]
pub async fn update_file(
    uuid: &str,
    user: AuthenticatedUser,
    mut form: Form<UpdateFileRequest<'_>>,
    mongo: &rocket::State<MongoDb>,
    config: &rocket::State<MyConfig>,
) -> Result<status::NoContent, CustomResponse> {
    let config = config.inner();
    let db = &mongo.database;
    let collection = db.collection::<File>("files");
    let metadata = match collection.find_one(doc! { "_id": ObjectId::from_str(uuid).unwrap() }, None).await {
        Ok(metadata) => metadata,
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    };
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => return Err(ApiError::NotFound("File not found".to_string().into()).to_response()),
    };
    if metadata.owner != user.uuid {
        return Err(ApiError::Forbidden("Permission denied".to_string().into()).to_response());
    }
    match metadata.type_ {
        FileType::File => {}
        _ => {
            return Err(ApiError::BadRequest("Target is not a file".to_string().into()).to_response());
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
        return Err(ApiError::BadRequest("Hash not match".to_string().into()).to_response());
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
