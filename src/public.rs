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
}

pub type CustomResponse = status::Custom<Json<ErrorResponse>>;

pub fn mongo_error_check<T>(result: Result<Option<T>,mongodb::error::Error>,document_name: &str) -> Result<T, CustomResponse> {
    match result {
        Ok(result) => match result {
            Some(result) => Ok(result),
            None => Err(ApiError::NotFound(Some(format!("{} not found", document_name))).to_response()),
        },
        Err(_) => Err(ApiError::InternalServerError(None).to_response()),
    }
}