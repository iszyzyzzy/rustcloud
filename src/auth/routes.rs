use super::lib::{authenticate, generate_jwt};
use super::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{LoginedDevice, LoginedDeviceType};
use crate::MyConfig;
use crate::libs::ApiError;
use chrono::Utc;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use rocket::form::Form;
use rocket::futures::TryStreamExt;
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

#[post("/login", data = "<user>")]
pub async fn login(
    user: Form<LoginRequest>,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    config: &rocket::State<MyConfig>,
) -> Result<Json<LoginResponse>, ApiError> {
    let user = user.into_inner();
    let auth_result = authenticate(&user.username, &user.password, mongo).await;
    match auth_result {
        Ok(login_user) => {
            let (token, jti) = generate_jwt(&login_user.uuid, &config.jwt_secret);
            redis.set(&token, &login_user.uuid.to_string()).await;
            redis.expire(&token, 4 * 60 * 60).await;
            let login_device = LoginedDevice {
                user_uuid: login_user.uuid.clone(),
                uuid: jti,
                name: user.device_name,
                logined_at: chrono::Utc::now(),
                expire_at: chrono::Utc::now() + chrono::Duration::hours(4),
                _id: ObjectId::new(),
                type_: LoginedDeviceType::Normal,
            };
            let db = mongo.database.collection::<LoginedDevice>("logined_devices");
            let _ = db.insert_one(login_device).await;
            return Ok(Json(LoginResponse {
                uuid: login_user.uuid.to_string(),
                username: login_user.username,
                nickname: login_user.nickname,
                token,
            }));
        }
        Err(_) => {
            return Err(
                ApiError::Unauthorized("Invalid username or password".to_string().into()),
            );
        }
    }
}

#[post("/logout")]
pub async fn logout(
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
) -> Result<status::NoContent, ApiError> {
    match user.token {
        Some(token) => {
            redis.delete(token.as_str()).await;
            let db = mongo.database.collection::<LoginedDevice>("logined_devices");
            let _ = db.delete_one(doc! { "uuid": token }).await;
        }
        None => {
            return Err(
                ApiError::Unauthorized("Invalid token".to_string().into()),
            );
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
pub async fn create_access_key(
    user: AuthenticatedUser,
    device_name: String,
    redis: &rocket::State<Redis>,
    mongo: &rocket::State<MongoDb>
) -> Result<Json<AccessKeyResponse>, ApiError> {
    let token = uuid::Uuid::new_v4().to_string();
    redis.set(&token, &user.uuid.to_string()).await;
    redis.expire(&token, 4 * 60 * 60).await;
    let db = mongo.database.collection::<LoginedDevice>("logined_devices");
    let _ = db.insert_one(LoginedDevice {
        user_uuid: user.uuid,
        uuid: token.clone(),
        name: device_name,
        logined_at: Utc::now(),
        expire_at: Utc::now() + chrono::Duration::days(365),
        _id: ObjectId::new(),
        type_: LoginedDeviceType::ApiKey,
    }).await;
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
) -> Result<status::NoContent, ApiError> {
    let db = mongo.database.collection::<LoginedDevice>("logined_devices");
    let _ = db.delete_one(doc! { "uuid": request.token.clone(), "user_uuid": user.uuid }).await;
    redis.delete(request.token.as_str()).await;
    Ok(status::NoContent)
}

#[get("/list_devices")]
pub async fn list_devices(
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
) -> Result<Json<Vec<LoginedDevice>>, ApiError> {
    let db = mongo.database.collection::<LoginedDevice>("logined_devices");
    let devices = db
        .find(doc! { "user_uuid": user.uuid })
        .await;
    let devices = match devices {
        Ok(devices) => {
            let devices = match devices.try_collect().await {
                Ok(devices) => devices,
                Err(_) => {
                    return Err(
                        ApiError::InternalServerError("DB error".to_string().into()),
                    );
                }
            };
            devices
        },
        Err(_) => {
            return Err(
                ApiError::InternalServerError("DB error".to_string().into()),
            );
        }
    };

    Ok(Json(devices))
}