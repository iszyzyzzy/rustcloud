use std::path::Path;

use super::lib::{SaveResult, StorageBackend, StorageConfig};
use crate::db::models::File;
use crate::libs::ApiError;
use async_trait::async_trait;
use rocket::fs::TempFile;
use rocket::tokio::fs;
use rocket::tokio::fs::File as AsyncFile;
use rocket::tokio::io::{AsyncReadExt, BufReader};
use sha2::{Digest, Sha256};

async fn file_sha256(file: AsyncFile) -> String {
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

async fn check_file_sha256(file_path: &Path, sha256: &str) -> Result<String, ()> {
    let file = AsyncFile::open(file_path).await.unwrap();
    let hash = file_sha256(file).await;
    if hash != sha256 {
        Err(())
    } else {
        Ok(hash)
    }
}

fn generate_file_path(metadata: &File, config: &StorageConfig) -> String {
    format!("{}/{}", config.flat_storage_path, metadata._id)
}

pub struct LocalFlatStorageBackend {
    config: StorageConfig,
}

#[async_trait]
impl StorageBackend for LocalFlatStorageBackend {
    fn new(config: &StorageConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
    async fn save_file(
        &self,
        metadata: &File,
        file:&mut TempFile<'_>,
    ) -> Result<SaveResult, ApiError> {
        let file_path = generate_file_path(metadata, &self.config);
        /*         match file.open(128.kibibytes()).into_file(file_path.clone()).await {
            Ok(_) => Ok(SaveResult {size: fs::metadata(file_path.clone()).await.unwrap().len(), _path: file_path.clone()}),
            Err(_) => Err(ApiError::InternalServerError(None)),
        } */
        match file.persist_to(&file_path).await {
            Ok(_) => Ok(SaveResult {
                size: fs::metadata(&file_path).await.unwrap().len(),
                _path: file_path.clone(),
                sha256: metadata.sha256.clone(),
            }),
            Err(_) => Err(ApiError::InternalServerError("Failed to save file".to_string().into())),
        }
    }

    async fn check_sha256_and_save(
        &self,
        metadata: &File,
        sha256: &str,
        file:&mut  TempFile<'_>,
    ) -> Result<SaveResult, ApiError> {
        let file_path = generate_file_path(metadata, &self.config);

        match check_file_sha256(file.path().unwrap(), sha256).await {
            Ok(_) => {}
            Err(_) => {
                return Err(ApiError::BadRequest("Hash not match".to_string().into()));
            }
        }
        match file.persist_to(&file_path).await {
            Ok(_) => Ok(SaveResult {
                size: fs::metadata(&file_path).await.unwrap().len(),
                _path: file_path.clone(),
                sha256: sha256.to_string(),
            }),
            Err(err) => {dbg!(err);
                Err(ApiError::InternalServerError("Failed to save file".to_string().into()))},
        }
    }

    async fn get_file(&self, metadata: &File) -> Result<AsyncFile, ApiError> {
        let file_path = generate_file_path(metadata, &self.config);
        Ok(
            AsyncFile::open(file_path)
                .await
                .unwrap(),
        )
    }
    async fn delete_file(&self, metadata: &File) -> Result<(), ApiError> {
        let file_path = generate_file_path(metadata, &self.config);
        let _ = fs::remove_file(file_path).await;
        Ok(())
    }
}
