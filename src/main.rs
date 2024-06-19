#[macro_use] extern crate rocket;

use mongodb::bson::oid::ObjectId;
use rocket::{figment::{providers::{Env, Format, Toml}, Figment}, serde::json::Json};

mod auth;
mod db;
mod file;
mod file_metadata;
mod public;

#[derive(serde::Deserialize)]
pub struct TempConfig {
    pub jwt_secret: String,
    pub mongodb_uri: String,
    pub mongodb_name: String,
    pub redis_uri: String,
    pub storage_path: String,
    pub port: u16,
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
    pub storage_path: String,
    pub port: u16,
    pub system_root_id: ObjectId,
}

impl MyConfig {
    fn from_temp(root_id: ObjectId,old:&TempConfig) -> Self {
        Self {
            jwt_secret: old.jwt_secret.clone(),
            mongodb_uri: old.mongodb_uri.clone(),
            mongodb_name: old.mongodb_name.clone(),
            redis_uri: old.redis_uri.clone(),
            storage_path: old.storage_path.clone(),
            port: old.port,
            system_root_id: root_id
        }
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
    let config = MyConfig::from_temp(root_id, &config);
    let redis = db::connect::Redis::init(&config.redis_uri).await;
    rocket::build()
        .manage(config)
        .manage(mongodb)
        .manage(redis)
        .mount("/", routes![index])
        .mount("/auth", routes![auth::routes::login])
        .mount("/auth", routes![auth::routes::logout])
        .mount("/auth", routes![auth::routes::create_access_key])
        .mount("/auth", routes![auth::routes::delete_access_key])
        .mount("/metadata", routes![file_metadata::routes::add_metadata])
        .mount("/metadata", routes![file_metadata::routes::get_metadata])
        .mount("/metadata", routes![file_metadata::routes::update_metadata])
        .mount("/metadata", routes![file_metadata::routes::delete_metadata])
        .mount("/file", routes![file::routes::get_file])
        .mount("/file", routes![file::routes::update_file])
        .mount("/file", routes![file::routes::upload_file])
}