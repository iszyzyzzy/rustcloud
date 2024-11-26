use super::guard::AuthenticatedUser;
use crate::db::connect::{MongoDb, Redis};
use crate::db::models::{File, User};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub jti: String,
}

pub fn generate_jwt(user_id: &ObjectId, jwt_secret: &str) -> (String, String) {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(4))
        .expect("valid timestamp")
        .timestamp();
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        sub: user_id.to_string(),
        exp: expiration,
        jti: jti.clone(),
    };

    (
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(jwt_secret.as_ref()),
        )
        .expect("JWT token creation failed"),
        jti,
    )
}

pub fn decode_jwt(token: &str, jwt_secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_ref()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
}

pub async fn authenticate(
    name: &str,
    password: &str,
    mongo: &rocket::State<MongoDb>,
) -> Result<AuthenticatedUser, Box<dyn std::error::Error + Send + Sync>> {
    let db = &mongo.database;
    let collection = db.collection::<User>("users");
    let user = collection
        .find_one(doc! { "username": name.to_string() })
        .await?;
    match user {
        Some(user) => {
            let argon2 = Argon2::default();
            let password_hash = PasswordHash::new(&user.password).unwrap();
            if argon2
                .verify_password(password.as_bytes(), &password_hash)
                .is_ok()
            {
                Ok(AuthenticatedUser {
                    uuid: user._id,
                    username: user.username,
                    nickname: user.nickname,
                    token: None,
                    root_id: user.root_id,
                })
            } else {
                Err("Invalid password".into())
            }
        }
        None => {
            Err("User not found".into())
        }
    }
}

pub async fn authenticate_jwt(
    token: &str,
    jwt_secret: &str,
    mongo: &MongoDb,
    redis: &Redis,
) -> Result<AuthenticatedUser, Box<dyn std::error::Error>> {
    match decode_jwt(token, jwt_secret) {
        Ok(claims) => {
            if claims.exp < Utc::now().timestamp() {
                return Err("Token expired".into());
            }
            if !redis.exists(&claims.jti).await {
                return Err("Invalid token".into());
            }
            let db = &mongo.database;
            let collection = db.collection::<User>("users");
            let user = collection
                .find_one(doc! { "uuid": claims.sub })
                .await
                .unwrap();
            match user {
                Some(user) => {
                    Ok(AuthenticatedUser {
                        uuid: user._id,
                        username: user.username,
                        nickname: user.nickname,
                        token: claims.jti.to_owned().into(),
                        root_id: user.root_id,
                    })
                }
                None => Err("User not found".into()),
            }
        }
        Err(_) => Err("Invalid jwt token".into()),
    }
}

pub async fn create_user(
    name: &str,
    password: &str,
    nickname: &str,
    mongo: &MongoDb,
    root_id: &ObjectId
) -> Result<(), Box<dyn std::error::Error>> {
    let db = &mongo.database;
    let collection = db.collection::<User>("users");
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = argon2.hash_password(password.as_bytes(), &salt).unwrap();
    let user_root_id = ObjectId::new();
    let user = User {
        username: name.to_string(),
        nickname: nickname.to_string(),
        password: password_hash.to_string(),
        _id: ObjectId::new(),
        root_id: user_root_id,
    };
    collection.insert_one(&user).await?;
    let collection = db.collection::<File>("files");
    collection
        .insert_one(
            File::new_folder(
                format!("{}_home", user.username).as_str(),
                root_id,
                &user._id,
                Some(user_root_id),
            )
        )
        .await?;
    Ok(())
}
