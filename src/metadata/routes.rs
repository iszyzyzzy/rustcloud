use serde::{Serialize, Deserialize};
use crate::db::models::FileType;

#[derive(Debug, Serialize, Deserialize)]
struct MetaDataCreateRequest {
    pub name: String,
    pub type_: FileType,
    pub father: String
    
}

#[post("/metadata", data = "<metadata>")]
pub async fn add_metadata(

) -> Result<status::NoContent, status::Custom<Json<ErrorResponse>>> {

}