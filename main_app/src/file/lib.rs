use crate::db::models::File;
use rocket::response;
use rocket::response::Response;
use rocket::response::Responder;
use rocket::tokio::io::AsyncReadExt;
use rocket::Request;
use rocket::http::Header;

use super::storage_backend::lib::StorageFactory;
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
    pub async fn new(metadata: File, factory: &rocket::State<Arc<Mutex<StorageFactory>>>) -> Self {
        let factory = factory.lock().await;
        let ext = rocket::http::ContentType::from_extension(metadata.name.split('.').last().unwrap());
        let file = factory.get_file(&metadata).await.unwrap();
        let mut response = Response::build();
        response
            .header(if let Some(ext) = ext { ext } else { rocket::http::ContentType::Binary })
            .header(Header::new("Content-Disposition", format!("attachment; filename=\"{}\"", metadata.name)))
            .streamed_body(file);
        Self { response: response.finalize() }
        
    }
}

pub async fn get_file_type(file: &rocket::fs::TempFile<'_>) -> Option<infer::Type> {
    let mut stream = file.open().await.unwrap();
    let mut buf = [0;512];
    stream.read(&mut buf).await.unwrap();
    infer::get(&buf)
}