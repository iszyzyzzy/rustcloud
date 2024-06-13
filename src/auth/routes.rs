use super::lib::{authenticate, generate_jwt};
use super::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::LoginedDevice;
use crate::MyConfig;
use chrono::Utc;
use mongodb::bson::doc;
use rocket::form::Form;
use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;

#[derive(FromForm,Debug)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub device_name: String,
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
pub async fn login(
    user: Form<LoginRequest>,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    config: &rocket::State<MyConfig>,
) -> Result<Json<LoginResponse>, status::Custom<Json<ErrorResponse>>> {
    let user = user.into_inner();
    let auth_result = authenticate(&user.username, &user.password, mongo).await;
    match auth_result {
        Ok(login_user) => {
            let (token, jti) = generate_jwt(&login_user.uuid, &config.jwt_secret);
            redis.set(&token, &login_user.uuid).await;
            redis.expire(&token, 4 * 60 * 60).await;
            let login_device = LoginedDevice {
                user_uuid: login_user.uuid.clone(),
                uuid: jti,
                name: user.device_name,
                logined_at: Utc::now().timestamp(),
                expire_at: Utc::now().timestamp() + 4 * 60 * 60,
                _id: uuid::Uuid::new_v4().to_string(),
            };
            let db = mongo.database.collection::<LoginedDevice>("logined_devices");
            let _ = db.insert_one(login_device, None).await;
            return Ok(Json(LoginResponse {
                uuid: login_user.uuid,
                username: login_user.username,
                nickname: login_user.nickname,
                token,
            }));
        }
        Err(info) => {
            return Err(status::Custom(
                Status::Unauthorized,
                Json(ErrorResponse {
                    error: "Invalid username or password".to_string(),
                }),
            ));
        }
    }
}

#[post("/logout")]
pub async fn logout(
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
) -> Result<status::NoContent, status::Custom<Json<ErrorResponse>>> {
    match user.token {
        Some(token) => {
            redis.delete(token.as_str()).await;
            let db = mongo.database.collection::<LoginedDevice>("logined_devices");
            let _ = db.delete_one(doc! { "uuid": token }, None).await;
        }
        None => {
            return Err(status::Custom(
                Status::Unauthorized,
                Json(ErrorResponse {
                    error: "Invalid token".to_string(),
                }),
            ));
        }
    }
    Ok(status::NoContent)
}

#[derive(serde::Serialize)]
pub struct AccessKeyResponse {
    pub token: String,
}

//长效token
#[get("/access_key?<device_name>")]
pub async fn access_key(
    user: AuthenticatedUser,
    device_name: String,
    redis: &rocket::State<Redis>,
    mongo: &rocket::State<MongoDb>
) -> Result<Json<AccessKeyResponse>, status::Custom<Json<ErrorResponse>>> {
    let token = uuid::Uuid::new_v4().to_string();
    redis.set(&token, &user.uuid).await;
    redis.expire(&token, 4 * 60 * 60).await;
    let db = mongo.database.collection::<LoginedDevice>("logined_devices");
    let _ = db.insert_one(LoginedDevice {
        user_uuid: user.uuid,
        uuid: token.clone(),
        name: device_name,
        logined_at: Utc::now().timestamp(),
        expire_at: 0,
        _id: uuid::Uuid::new_v4().to_string(),
    }, None).await;
    Ok(Json(AccessKeyResponse { token }))
}

#[derive(serde::Deserialize)]
pub struct DeleteAccessKeyRequest {
    pub token: String,
}

#[delete("/access_key",data = "<request>")]
pub async fn delete_access_key(
    user: AuthenticatedUser,
    request: Json<DeleteAccessKeyRequest>,
    redis: &rocket::State<Redis>,
    mongo: &rocket::State<MongoDb>
) -> Result<status::NoContent, status::Custom<Json<ErrorResponse>>> {
    let db = mongo.database.collection::<LoginedDevice>("logined_devices");
    let _ = db.delete_one(doc! { "uuid": request.token.clone(), "user_uuid": user.uuid }, None).await;
    redis.delete(request.token.as_str()).await;
    Ok(status::NoContent)
}