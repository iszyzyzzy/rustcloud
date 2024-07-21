#[macro_use] extern crate rocket;

use std::{net::IpAddr, sync::Arc};

use mongodb::bson::oid::ObjectId;
use rocket::{figment::{providers::{Env, Format, Toml}, Figment}, serde::json::Json, tokio::sync::Mutex};
use crate::file::storage_backend::lib::StorageBackend;

mod libs;
mod auth;
mod db;
mod file;
mod file_metadata;

#[derive(serde::Deserialize)]
struct TempConfig {
    jwt_secret: String,
    mongodb_uri: String,
    mongodb_name: String,
    redis_uri: String,
    flat_storage_path: String,
    cache_storage_path: String,
    port: u16,
    address: IpAddr
}

impl TempConfig {
    fn from_env() -> Self {
        dotenv::dotenv().ok();
        let figment = Figment::new()
            .merge(Env::prefixed("RC_"))
            .join(Toml::file("defaultConfig.toml"));
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
    pub address: IpAddr
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
            address: old.address
        }
    }
}

struct AppConfig {
    port: u16,
    temp_dir: String,
    address: IpAddr
}

impl AppConfig {
    fn from_config(config: &MyConfig) -> Self {
        Self {
            port: config.port,
            temp_dir: config.cache_storage_path.clone(),
            address: config.address
        }
    }
    fn to_figment(&self) -> Figment {
        Figment::from(rocket::Config::default())
            .merge(("port", self.port))
            .merge(("temp_dir", self.temp_dir.clone()))
            .merge(("address", self.address))
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

#[launch]
async fn rocket() -> _ {
    let config = TempConfig::from_env();
    let mongodb = db::connect::MongoDb::init(&config.mongodb_uri, &config.mongodb_name).await;
    let root_id = mongodb.first_init().await.unwrap();
    let redis = db::connect::Redis::init(&config.redis_uri).await;

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