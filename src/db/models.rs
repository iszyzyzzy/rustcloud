use chrono::Utc;
use serde::{Serialize, Deserialize};
use mongodb::bson::oid::ObjectId;


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub _id: ObjectId,
    pub username: String,
    pub nickname: String,
    pub password: String,
    pub root_id: ObjectId,
}

use mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum LoginedDeviceType {
    Normal,
    ApiKey,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginedDevice {
    pub _id: ObjectId,
    pub name: String,
    #[serde(with = "chrono_datetime_as_bson_datetime")]
    pub logined_at: chrono::DateTime<chrono::Utc>,
    #[serde(with = "chrono_datetime_as_bson_datetime")]
    pub expire_at: chrono::DateTime<chrono::Utc>,
    pub uuid: String, //jti or apikey
    pub user_uuid: ObjectId,
    pub type_: LoginedDeviceType,
}


#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum FileType {
    File,
    Folder,
    Root,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct File {
    pub _id: ObjectId,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: FileType,
    pub father: ObjectId,
    pub children: Vec<ObjectId>,
    pub owner: ObjectId,
    pub created_at: i64,
    pub updated_at: i64,
    pub size: u64,//floder时可以随便填
    pub sha256: String,//floder时可以随便填
    pub path: String,//floder时可以随便填
    pub storage_type: String,
}

impl File {
    pub fn new_folder(name: &str, father: &ObjectId, owner: &ObjectId, id: Option<ObjectId>) -> Self {
        Self {
            _id: id.unwrap_or(ObjectId::new()),
            name: name.to_string(),
            type_: FileType::Folder,
            father: father.clone(),
            children: vec![],
            owner: owner.clone(),
            created_at: Utc::now().timestamp(),
            updated_at: Utc::now().timestamp(),
            size: 0,
            sha256: "".to_string(),
            path: "".to_string(),
            storage_type: "FLAT".to_string(),
        }
    }
}