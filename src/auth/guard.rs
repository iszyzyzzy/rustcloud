use std::str::FromStr;

use super::lib::authenticate_jwt;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::User;
use crate::MyConfig;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome};
use rocket::Request;

pub struct AuthenticatedUser {
    pub uuid: ObjectId,
    pub username: String,
    pub nickname: String,
    pub token: Option<String>,
    pub root_id: ObjectId,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let mongo = request.rocket().state::<MongoDb>().unwrap();
        let redis = request.rocket().state::<Redis>().unwrap();
        let config = request.rocket().state::<MyConfig>().unwrap();
        if request.headers().get_one("Authorization").is_none() {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        let auth_header = request.headers().get_one("Authorization").unwrap();
        if !auth_header.starts_with("Bearer ") {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        let token = &auth_header[7..];
        //先匹配access_key
        if redis.exists(token).await {
            let db = mongo.database.collection::<User>("users");
            let user = db
                .find_one(doc! { "_id": ObjectId::from_str(redis.get::<String, String>(token.to_string()).await.as_str()).unwrap() })
                .await;
            match user {
                Ok(Some(user)) => {
                        return Outcome::Success(AuthenticatedUser {
                            uuid: user._id,
                            username: user.username,
                            nickname: user.nickname,
                            token: Some(token.to_owned()),
                            root_id: user.root_id,
                        })
                    }
                Ok(None) => return Outcome::Error((Status::Unauthorized, ())),
                _ => return Outcome::Error((Status::Unauthorized, ())),
            }
        }

        match authenticate_jwt(token, &config.jwt_secret, mongo, redis).await {
            Ok(user) => return Outcome::Success(user),
            Err(_) => return Outcome::Error((Status::Unauthorized, ())),
        }
    }
}
