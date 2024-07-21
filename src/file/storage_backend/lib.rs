use std::collections::HashMap;
use async_trait::async_trait;
use rocket::fs::TempFile;
use rocket::tokio::fs::File as AsyncFile;
use crate::libs::ApiError;
use crate::db::models::File;
use crate::MyConfig;

#[derive(Clone)]
pub struct StorageConfig {
    pub flat_storage_path: String,
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    fn new(config: &StorageConfig) -> Self
    where
        Self: Sized;

    async fn save_file(&self, metadata: &File, file:&mut  TempFile<'_>) -> Result<SaveResult, ApiError>;
    async fn check_sha256_and_save(&self, metadata: &File, sha256: &str, file:&mut TempFile<'_>) -> Result<SaveResult, ApiError>;
    async fn get_file(&self, metadata: &File) -> Result<AsyncFile, ApiError>;
    async fn delete_file(&self, metadata: &File) -> Result<(), ApiError>;
}

pub struct SaveResult {
    pub size: u64,
    pub _path: String,
    pub sha256: String
}

pub struct StorageFactory {
    pub config: StorageConfig,
    backends: HashMap<String, Box<dyn StorageBackend>>
}


impl StorageFactory {
    pub fn new(config: &MyConfig) -> Self {
        let config = StorageConfig {
            flat_storage_path: config.flat_storage_path.clone()
        };
        Self { 
            config,
            backends: HashMap::new()
        }
    }

    pub fn get_config(&self) -> &StorageConfig {
        &self.config
    }

    pub fn register_backend(&mut self, name: &str, backend: Box<dyn StorageBackend>) {
        self.backends.insert(name.to_string(), backend);
    }

    pub fn get_backend(&self, name: &str) -> Option<&Box<dyn StorageBackend>> {
        self.backends.get(name)
    }

    pub fn get_backend_check(&self, name: &str) -> Result<&Box<dyn StorageBackend>, ApiError> {
        match self.backends.get(name) {
            Some(backend) => Ok(backend),
            None => Err(ApiError::InternalServerError("Storage backend not found".to_string().into()))
        }
    }

    pub async fn get_file(&self, metadata: &File) -> Result<AsyncFile, ApiError> {
        let backend = self.get_backend_check(&metadata.storage_type)?;
        backend.get_file(metadata).await
    }

    pub async fn save_file(&self, metadata: &File, file: &mut TempFile<'_>) -> Result<SaveResult, ApiError> {
        let backend = self.get_backend_check(&metadata.storage_type)?;
        backend.save_file(metadata, file).await
    }

    pub async fn check_sha256_and_save(&self, metadata: &File, sha256: Option<&str>, file:&mut TempFile<'_>) -> Result<SaveResult, ApiError> {
        let backend = self.get_backend_check(&metadata.storage_type)?;
        let sha256 = sha256.unwrap_or(&metadata.sha256);
        backend.check_sha256_and_save(metadata, sha256, file).await
    }

    pub async fn delete_file(&self, metadata: &File) -> Result<(), ApiError> {
        let backend = self.get_backend_check(&metadata.storage_type)?;
        backend.delete_file(metadata).await
    }
}