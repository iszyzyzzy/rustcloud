use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::form::Form;
use mongodb::bson::doc;
use crate::db::connect::{MongoDb, Redis};
use super::lib::{authenticate,generate_jwt};
use crate::MyConfig;

#[derive(FromForm)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub uuid: String,
    pub username: String,
    pub nickname: String,
    pub token: String,
}


#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
}


#[post("/login", data = "<user>")]
pub async fn login(user: Form<LoginRequest>, mongo: &rocket::State<MongoDb>, redis: &rocket::State<Redis>, config: &rocket::State<MyConfig>) -> Result<Json<LoginResponse>, status::Custom<Json<ErrorResponse>>> {
    let user = user.into_inner();
    let auth_result = authenticate(&user.username, &user.password, mongo).await;
    match auth_result {
        Ok(user) => {
            let token = generate_jwt(&user.uuid, &config.jwt_secret);
            redis.set(&user.uuid, &token).await;
            return Ok(Json(LoginResponse { uuid: user.uuid, username: user.username, nickname: user.nickname, token }));
        }
        Err(_) => {
            return Err(status::Custom(Status::Unauthorized, Json(ErrorResponse { error: "Invalid username or password".to_string() })));
        }
    }
}