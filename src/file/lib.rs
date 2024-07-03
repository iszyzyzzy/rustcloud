use rocket::data::{Data, ToByteUnit};

use crate::public::ApiError;

pub async fn receive_file(file_path: &str, file: Data<'_>) -> Result<(), ApiError> {
    match file.open(128.kibibytes()).into_file(file_path).await {
        Ok(_) => Ok(()),
        Err(_) => Err(ApiError::InternalServerError(None)),
    }
}

pub fn delete_file(path: &str) {
    let _ = std::fs::remove_file(path);
}

use rocket::tokio::fs::File as AsyncFile;
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

pub async fn check_file_sha256(file_path: &str, sha256: &str) -> Result<String, ()> {
    let hash = file_sha256(file_path).await;
    if hash != sha256 {
        Err(())
    } else {
        Ok(hash)
    }
}