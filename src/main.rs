#[macro_use] extern crate rocket;

use rocket::figment::{providers::{Env, Format, Toml}, Figment};

mod auth;
mod db;

#[derive(serde::Deserialize)]
pub struct MyConfig {
    pub jwt_secret: String,
    pub mongodb_uri: String,
    pub mongodb_name: String,
    pub redis_uri: String,
    pub storage_path: String,
    pub port: u16,
}

impl MyConfig {
    fn from_env() -> Self {
        dotenv::dotenv().ok();
        let figment = Figment::new()
            .merge(Env::prefixed("RC_"))
            .join(Toml::file("defaultConfig.toml"));
        figment.extract().unwrap()
    }
}

#[launch]
async fn rocket() -> _ {
    let config = MyConfig::from_env();
    let mongodb = db::connect::MongoDb::init(&config.mongodb_uri, &config.mongodb_name).await;
    let _ = mongodb.first_init().await;
    let redis = db::connect::Redis::init(&config.redis_uri).await;
    rocket::build()
        .manage(config)
        .manage(mongodb)
        .manage(redis)
        .mount("/auth", routes![auth::routes::login])
        .mount("/auth", routes![auth::routes::logout])
        .mount("/auth", routes![auth::routes::access_key])
        .mount("/auth", routes![auth::routes::delete_access_key])

}