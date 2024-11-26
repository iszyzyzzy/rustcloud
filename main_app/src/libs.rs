#[derive(serde::Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;

#[derive(Debug)]
pub enum ApiError {
    InternalServerError(Option<String>),
    NotFound(Option<String>),
    Unauthorized(Option<String>),
    Forbidden(Option<String>),
    BadRequest(Option<String>),
}

impl ApiError {
    pub fn to_response(&self) -> status::Custom<Json<ErrorResponse>> {
        match self {
            ApiError::InternalServerError(message) => status::Custom(
                Status::InternalServerError,
                Json(ErrorResponse {
                    error: message.clone().unwrap_or_else(|| "Internal Server Error".to_string()),
                }),
            ),
            ApiError::NotFound(message) => status::Custom(
                Status::NotFound,
                Json(ErrorResponse {
                    error: message.clone().unwrap_or_else(|| "Not Found".to_string()),
                }),
            ),
            ApiError::Unauthorized(message) => status::Custom(
                Status::Unauthorized,
                Json(ErrorResponse {
                    error: message.clone().unwrap_or_else(|| "Unauthorized".to_string()),
                }),
            ),
            ApiError::Forbidden(message) => status::Custom(
                Status::Forbidden,
                Json(ErrorResponse {
                    error: message.clone().unwrap_or_else(|| "Forbidden".to_string()),
                }),
            ),
            ApiError::BadRequest(message) => status::Custom(
                Status::BadRequest,
                Json(ErrorResponse {
                    error: message.clone().unwrap_or_else(|| "Bad Request".to_string()),
                }),
            ),
        }
    }
    pub fn _change_message(&self, message: String) -> Self {
        match self {
            ApiError::InternalServerError(_) => ApiError::InternalServerError(Some(message)),
            ApiError::NotFound(_) => ApiError::NotFound(Some(message)),
            ApiError::Unauthorized(_) => ApiError::Unauthorized(Some(message)),
            ApiError::Forbidden(_) => ApiError::Forbidden(Some(message)),
            ApiError::BadRequest(_) => ApiError::BadRequest(Some(message)),
        }
    }
    pub fn _to_string(&self) -> String {
        match self {
            ApiError::InternalServerError(message) => message.clone().unwrap_or_default(),
            ApiError::NotFound(message) => message.clone().unwrap_or_default(),
            ApiError::Unauthorized(message) => message.clone().unwrap_or_default(),
            ApiError::Forbidden(message) => message.clone().unwrap_or_default(),
            ApiError::BadRequest(message) => message.clone().unwrap_or_default(),
        }
    }
}

use rocket::request::Request;
use rocket::response::{self, Responder};

impl <'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, req: &Request<'_>) -> response::Result<'static> {
        self.to_response().respond_to(req)
    }
}

//pub type CustomResponse = status::Custom<Json<ErrorResponse>>;

pub fn mongo_error_check<T>(result: Result<Option<T>,mongodb::error::Error>,document_name: Option<&str>) -> Result<T, ApiError> {
    let document_name = document_name.unwrap_or("document");
    match result {
        Ok(Some(document)) => Ok(document),
        Ok(None) => Err(ApiError::NotFound(Some(format!("{} not found", document_name)))),
        //Err(err) => Err(ApiError::InternalServerError(Some(err.to_string()))),
        Err(_) => Err(ApiError::InternalServerError("MongoDB error".to_string().into())),
    }
}

use crate::auth::guard::AuthenticatedUser;
use crate::db::models::File;

pub fn check_file_permission(user: &AuthenticatedUser, file: &File) -> Result<(), ApiError> {
    if file.owner != user.uuid {
        return Err(
            ApiError::Forbidden("Permission denied".to_string().into()),
        )
    };
    Ok(())
}
