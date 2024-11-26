use crate::db::models::File;
use crate::libs::ApiError;
use crate::db::connect::MongoDb;
use mongodb::bson::doc;
use rocket::response;
use rocket::response::Response;
use rocket::response::Responder;
use rocket::tokio::io::AsyncReadExt;
use rocket::Request;
use rocket::http::Header;

use super::storage_backend::lib::StorageFactory;
use std::str::FromStr;
use std::sync::Arc;
use rocket::tokio::sync::Mutex;

pub struct CustomFileResponse {
    response: Response<'static>,
}

impl<'r> Responder<'r, 'static> for CustomFileResponse {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        Ok(self.response)
    }
}

impl CustomFileResponse {
    pub async fn new(metadata: File, factory: &rocket::State<Arc<Mutex<StorageFactory>>>, mongodb: &rocket::State<MongoDb>) -> Result<Self, ApiError> {
        let factory = factory.lock().await;
        let ext = rocket::http::ContentType::from_extension(metadata.name.split('.').last().unwrap());

        let metadata = if metadata.storage_type == "ref" {//因为storage backend并没有传入db实例，只能在这里处理ref了
            let collection = mongodb.database.collection::<File>("files");
            collection.find_one(doc! { "_id": mongodb::bson::oid::ObjectId::from_str(metadata.path.as_str()).unwrap() }).await.unwrap().unwrap()
        } else {
            metadata
        };

        let file =  factory.get_file(&metadata).await?;
        let mut response = Response::build();
        response
            .header(if let Some(ext) = ext { ext } else { rocket::http::ContentType::Binary })
            .header(Header::new("Content-Disposition", format!("attachment; filename=\"{}\"", metadata.name)))
            .streamed_body(file);
        Ok(Self { response: response.finalize() })
        
    }
}

pub async fn get_file_type(file: &rocket::fs::TempFile<'_>) -> Option<infer::Type> {
    let mut stream = file.open().await.unwrap();
    let mut buf = [0;512];
    stream.read(&mut buf).await.unwrap();
    infer::get(&buf)
}