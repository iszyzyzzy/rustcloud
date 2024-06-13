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