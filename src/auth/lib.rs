use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use mongodb::bson::doc;
use serde::{Serialize, Deserialize};
use chrono::{Utc, Duration};
use argon2::{
    password_hash::{
        rand_core::OsRng,
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString
    },
    Argon2
};
use uuid::Uuid;
use crate::db::connect::MongoDb;
use crate::db::connect::Redis;
use crate::db::models::User;
use super::guard::AuthenticatedUser;


#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub jti: String,
}

pub fn generate_jwt(user_id: &str,jwt_secret: &str) -> (String, String) {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(4))
        .expect("valid timestamp")
        .timestamp();
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        sub: user_id.to_owned(),
        exp: expiration,
        jti: jti.clone(),
    };

    (encode(&Header::default(), &claims, &EncodingKey::from_secret(jwt_secret.as_ref())).expect("JWT token creation failed"), jti)
}

pub fn decode_jwt(token: &str,jwt_secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(token, &DecodingKey::from_secret(jwt_secret.as_ref()), &Validation::new(Algorithm::HS256))
        .map(|data| data.claims)
}

pub async fn authenticate(name: &str, password: &str, mongo: &rocket::State<MongoDb>) -> Result<AuthenticatedUser, Box<dyn std::error::Error + Send + Sync>> {
    let db = &mongo.database;
    let collection = db.collection::<User>("users");
    let user = collection.find_one(doc! { "username": name.to_string() }, None).await?;
    match user {
        Some(user) => {
            let argon2 = Argon2::default();
            let password_hash = PasswordHash::new(&user.password).unwrap();
            if argon2.verify_password(password.as_bytes(), &password_hash).is_ok() {
                return Ok(AuthenticatedUser { uuid: user.uuid, username: user.username, nickname: user.nickname, token: None });
            } else {
                return Err("Invalid password".into());
            }
        }
        None => {
            return Err("User not found".into());
        }
    }
}

pub async fn authenticate_jwt(token: &str, jwt_secret: &str, mongo: &MongoDb, redis: &Redis) -> Result<AuthenticatedUser, Box<dyn std::error::Error>> {
    match decode_jwt(token,jwt_secret) {
        Ok(claims) => {
            if claims.exp < Utc::now().timestamp() {
                return Err("Token expired".into());
            }
            if !redis.exists(&claims.jti).await {
                return Err("Invalid token".into());
            }
            let db = &mongo.database;
            let collection = db.collection::<User>("users");
            let user = collection.find_one(doc! { "uuid": claims.sub }, None).await.unwrap();
            match user {
                Some(user) => return Ok(AuthenticatedUser { uuid: user.uuid, username: user.username, nickname: user.nickname,token: claims.jti.to_owned().into() }),
                None => return Err("User not found".into()),
            }
        },
        Err(_) => return Err("Invalid jwt token".into()),
    }
}

pub async fn create_user(name: &str, password: &str, nickname: &str, mongo: &MongoDb) -> Result<(), Box<dyn std::error::Error>> {
    let db = &mongo.database;
    let collection = db.collection::<User>("users");
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = argon2.hash_password(password.as_bytes(), &salt).unwrap();
    let user = User {
        uuid: Uuid::new_v4().to_string(),
        username: name.to_string(),
        nickname: nickname.to_string(),
        password: password_hash.to_string(),
        _id: uuid::Uuid::new_v4().to_string()
    };
    collection.insert_one(user, None).await?;
    Ok(())
}