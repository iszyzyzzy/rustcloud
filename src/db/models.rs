use serde::{Serialize, Deserialize};


#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub _id: String,
    pub uuid: String,
    pub username: String,
    pub nickname: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginedDevice {
    pub _id: String,
    pub name: String,
    pub logined_at: i64,
    pub expire_at: i64,
    pub uuid: String, //jti or apikey
    pub user_uuid: String,
}


#[derive(Debug, Serialize, Deserialize)]
pub enum FileType {
    File,
    Folder,
    Root,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct File {
    pub _id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: FileType,
    pub father: String,
    pub children: Vec<String>,
    pub owner: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub size: i64,
    pub md5: String,
    pub path: String,
}