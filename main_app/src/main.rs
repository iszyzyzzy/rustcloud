#[macro_use] extern crate rocket;

use std::{net::IpAddr, sync::Arc};

use mongodb::bson::oid::ObjectId;
use rocket::{figment::{providers::{Env, Format, Serialized, Toml}, Figment}, serde::json::Json, tokio::sync::Mutex};
use crate::file::storage_backend::lib::StorageBackend;

mod libs;
mod auth;
mod db;
mod file;
mod file_metadata;

use rocket::data::{Limits, ToByteUnit};

#[derive(serde::Deserialize, serde::Serialize)]
struct TempConfig {
    jwt_secret: String,
    mongodb_uri: String,
    mongodb_name: String,
    redis_uri: String,
    flat_storage_path: String,
    cache_storage_path: String,
    port: u16,
    address: IpAddr,
    limits: Limits
}

impl Default for TempConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "a-secret-key".to_string(),
            mongodb_uri: "mongodb://localhost:27017".to_string(),
            mongodb_name: "RC".to_string(),
            redis_uri: "redis://localhost:6379".to_string(),
            //flat_storage_path: "./storage/flat".to_string(),
            flat_storage_path: "./main_app/storage/flat".to_string(),
            //cache_storage_path: "./storage/cache".to_string(),
            cache_storage_path: "./main_app/storage/cache".to_string(),
            port: 8000,
            address: "0.0.0.0".parse().unwrap(),
            limits: Limits::default().limit("file", 4.gibibytes())
        }
    }
}

impl TempConfig {
    fn from_env() -> Self {
        dotenv::dotenv().ok();
        let figment = Figment::new()
            .merge(Toml::file("RC.toml"))
            .merge(Env::prefixed("RC_"))
            .join(Serialized::defaults(TempConfig::default()));
        figment.extract().unwrap()
    }
}

pub struct MyConfig {
    pub jwt_secret: String,
    pub mongodb_uri: String,
    pub mongodb_name: String,
    pub redis_uri: String,
    pub flat_storage_path: String,
    pub cache_storage_path: String,
    pub port: u16,
    pub system_root_id: ObjectId,
    pub address: IpAddr,
    pub limits: Limits
}

impl MyConfig {
    fn from_temp(root_id: ObjectId,old:&TempConfig) -> Self {
        Self {
            jwt_secret: old.jwt_secret.clone(),
            mongodb_uri: old.mongodb_uri.clone(),
            mongodb_name: old.mongodb_name.clone(),
            redis_uri: old.redis_uri.clone(),
            flat_storage_path: old.flat_storage_path.clone(),
            cache_storage_path: old.cache_storage_path.clone(),
            port: old.port,
            system_root_id: root_id,
            address: old.address,
            limits: old.limits.clone()
        }
    }
}

struct AppConfig {
    port: u16,
    temp_dir: String,
    address: IpAddr,
    limits: Limits
}

impl AppConfig {
    fn from_config(config: &MyConfig) -> Self {
        Self {
            port: config.port,
            temp_dir: config.cache_storage_path.clone(),
            address: config.address,
            limits: config.limits.clone()
        }
    }
    fn to_figment(&self) -> Figment {
        Figment::from(rocket::Config::default())
            .merge(("port", self.port))
            .merge(("temp_dir", self.temp_dir.clone()))
            .merge(("address", self.address))
            .merge(("limits", self.limits.clone()))
    }
}


#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BasicInfo {
    pub server_time: String,
    pub version: String,
    pub user_root_id: Option<String>,
    pub user_id: Option<String>,
}

#[get("/")]
fn index(
    user: Result<auth::guard::AuthenticatedUser, ()>,
    _config: &rocket::State<MyConfig>
) -> Json<BasicInfo> {
    let (user_root_id, user_id) = match user {
        Ok(user) => (Some(user.root_id.to_string()), Some(user.uuid.to_string())),
        Err(_) => (None, None)
    };
    let r = BasicInfo {
        server_time: chrono::Utc::now().to_string(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        user_root_id,
        user_id,
    };
    Json(r)
}

use crate::db::connect::{MongoDb, Redis};
use crate::db::FirstInit;

#[launch]
async fn rocket() -> _ {
    let config = TempConfig::from_env();
    let mut mongodb = MongoDb::init(&config.mongodb_uri, &config.mongodb_name).await;
    let _ = mongodb.first_init().await.unwrap();
    let root_id = mongodb.get_root_id().await.unwrap();
    let redis = Redis::init(&config.redis_uri).await;

    let config = MyConfig::from_temp(root_id, &config);
    let app_config = AppConfig::from_config(&config);

    let mut storage_factory = file::storage_backend::lib::StorageFactory::new(&config);
    storage_factory.register_backend("FLAT", Box::new(file::storage_backend::flat::LocalFlatStorageBackend::new(storage_factory.get_config())));

    rocket::custom(app_config.to_figment())
        .manage(config)
        .manage(mongodb)
        .manage(redis)
        .manage(Arc::new(Mutex::new(storage_factory)))
        .mount("/", routes![index])
        .mount("/auth", routes![
            auth::routes::login,
            auth::routes::logout,
            auth::routes::create_access_key,
            auth::routes::delete_access_key,
            auth::routes::list_devices,
        ])
        .mount("/metadata", routes![
            file_metadata::routes::add_metadata,
            file_metadata::routes::get_metadata,
            file_metadata::routes::update_metadata,
            file_metadata::routes::delete_metadata,
        ])
        .mount("/file", routes![
            file::routes::get_file,
            file::routes::update_file,
            file::routes::upload_file,
        ])
        .mount("/file/share", routes![
            file::share::routes::crate_share_link,
            file::share::routes::get_share_file,
        ])
}