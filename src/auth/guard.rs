use rocket::Request;
use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome};
use super::lib::authenticate_jwt;
use crate::db::connect::{MongoDb,Redis};
use crate::MyConfig;




pub struct AuthenticatedUser {
    pub uuid: String,
    pub username: String,
    pub nickname: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedUser {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let mongo = request.rocket().state::<MongoDb>().unwrap();
        let redis = request.rocket().state::<Redis>().unwrap();
        let config = request.rocket().state::<MyConfig>().unwrap();
        if let None = request.headers().get_one("Authorization") {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        let auth_header = request.headers().get_one("Authorization").unwrap();
        if !auth_header.starts_with("Bearer ") {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        let token = &auth_header[7..];
        match authenticate_jwt(token, &config.jwt_secret, mongo, redis).await {
            Ok(user) => return Outcome::Success(user),
            Err(_) => return Outcome::Error((Status::Unauthorized, ())),
        }
    }
}