use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub uuid: String,
    pub username: String,
    pub nickname: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginedDevice {
    pub name: String,
    pub logined_at: i64,
    pub expire_at: i64,
    pub _id: String, //jti or apikey
}