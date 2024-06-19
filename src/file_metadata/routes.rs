use std::str::FromStr;

use chrono::Utc;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use serde::{Serialize, Deserialize};
use rocket::response::status;
use rocket::serde::json::Json;
use crate::db::models::{FileType, File};
use crate::auth::guard::AuthenticatedUser;
use crate::public::{ApiError, CustomResponse,mongo_error_check};
use crate::db::connect::{Redis,MongoDb};
use crate::MyConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaDataCreateRequest {
    pub name: String,
    pub type_: FileType,
    pub father: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaDataCreateResponse {
    pub id: String,
}

//收到metadata的时候先存到redis里，等到收到文件再写入mongodb
//文件夹就直接写入
#[post("/create", data = "<metadata>")]
pub async fn add_metadata(
    metadata: Json<MetaDataCreateRequest>,
    user: AuthenticatedUser,
    redis: &rocket::State<Redis>,
    mongo: &rocket::State<MongoDb>
) -> Result<Json<MetaDataCreateResponse>, CustomResponse> {
    let id = ObjectId::new();
    let metadata = metadata.into_inner();
    let metadata = File {
        _id: id.clone(),
        name: metadata.name,
        type_: metadata.type_,
        father: ObjectId::from_str(&metadata.father).unwrap(),
        size: metadata.size,
        sha256: metadata.sha256,
        owner: user.uuid,
        created_at: Utc::now().timestamp(),
        updated_at: Utc::now().timestamp(),
        children: vec![],
        path: "FLAT".to_string()
    };
    let father = mongo.database.collection::<File>("files").find_one(doc! { "_id": metadata.father }, None).await;
    match father {
        Ok(Some(_)) => {}
        Ok(None) => return Err(ApiError::NotFound("Father folder not found".to_string().into()).to_response()),
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    }
    match metadata.type_ {
        FileType::Folder => {
            let db = mongo.database.collection::<File>("files");
            let _ = db.insert_one(metadata.clone(), None).await;
            let father = db.find_one(doc! { "_id": metadata.father }, None).await.unwrap().unwrap();
            let mut t = father.children;
            t.push(metadata._id);
            let _ = db.update_one(doc! { "_id": father._id }, doc! { "$set": { "children": t } }, None).await;
            return Ok(Json(MetaDataCreateResponse { id:id.to_string() }));
        },
        FileType::File => {
            let _ = redis.set(id.to_string().as_str(), serde_json::to_string(&metadata).unwrap().as_str()).await;
            let _ = redis.expire(id.to_string().as_str(), 24 * 60 * 60).await;
            Ok(Json(MetaDataCreateResponse { id:id.to_string() }))
        },
        _ => Err(ApiError::Forbidden("Permission denied".to_string().into()).to_response()),
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct FileTree {
    pub _id: ObjectId,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: FileType,
    pub father: ObjectId,
    pub children: Vec<FileTree>,
    pub owner: ObjectId,
    pub created_at: i64,
    pub updated_at: i64,
    pub size: u64,
    pub sha256: String,
    pub path: String,
}

async fn get_tree(root_id:&ObjectId,mongo:&MongoDb) -> Result<FileTree, Box<dyn std::error::Error + Send + Sync>> {
    let db = mongo.database.collection::<File>("files");
    let file = db.find_one(doc! {"_id": root_id}, None).await.unwrap();
    match file {
        Some(file) => {
            let mut children = vec![];
            for child_id in file.children {
                children.push(Box::pin(get_tree(&child_id, &mongo)).await?);
            }
            let tree = FileTree {
                _id: file._id,
                name: file.name,
                type_: file.type_,
                father: file.father,
                children: children,
                owner: file.owner,
                created_at: file.created_at,
                updated_at: file.updated_at,
                size: file.size,
                sha256: file.sha256,
                path: file.path
            };
            return Ok(tree);
        }
        None => {
            return Err("File not found".into());
        }
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    File(File),
    FileTree(FileTree),
}

fn check_permission(user: AuthenticatedUser, file: &File) -> Result<(), CustomResponse> {
    if file.owner != user.uuid {
        return Err(
            ApiError::Forbidden("Permission denied".to_string().into()).to_response(),
        )
    };
    Ok(())
}

#[get("/<uuid>?<tree>")]
pub async fn get_metadata(
    uuid: &str,
    tree: Option<bool>,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
) -> Result<Json<Response>, CustomResponse> {
    let tree = tree.unwrap_or(false);
    let db = mongo.database.collection::<File>("files");
    let file = db.find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()}, None).await;
    let file = mongo_error_check(file, "File")?;
    let _ = check_permission(user, &file)?;
    if tree {
        let tree = get_tree(&ObjectId::from_str(uuid).unwrap(), &mongo).await;
        match tree {
            Ok(tree) => {
                return Ok(Json(Response::FileTree(tree)));
            }
            Err(_) => {
                return Err(
                    ApiError::InternalServerError(None).to_response(),
                )
            }
        }
    }
    Ok(Json(Response::File(file)))
}


#[put("/<uuid>", data = "<metadata>")]
pub async fn update_metadata(
    uuid: &str,
    metadata: Json<File>,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
) -> Result<status::NoContent, CustomResponse> {
    let new_metadata = metadata.into_inner();
    let db = mongo.database.collection::<File>("files");
    let file = db.find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()}, None).await;
    let file = mongo_error_check(file, "File")?;
    //其实只有father,name可以更新
    //sha256和size是在文件更新的时候改的
    //path,type不能更新
    //created_at和updated_at是由系统指定的
    //owner肯定不能动
    //查一下
    if new_metadata.sha256 != file.sha256 || 
        new_metadata.size != file.size ||
        new_metadata.type_ != file.type_ ||
        new_metadata.path != file.path ||
        new_metadata.owner != file.owner ||
        new_metadata.created_at != file.created_at {
        return Err(ApiError::BadRequest("Only father and name can be updated".to_string().into()).to_response());
    };
    //再直接覆写掉updated_at
    let new_metadata = File {
        updated_at: Utc::now().timestamp(),
        ..new_metadata
    };
    let _ = check_permission(user, &file)?;
    let old_father = match db.find_one(doc! {"_id": file.father}, None).await {
        Ok(Some(file)) => file,
        Ok(None) => return Err(ApiError::InternalServerError(None).to_response()),//这是不应该发生的情况
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    };
    let new_father = match db.find_one(doc! {"_id": new_metadata.father}, None).await {
        Ok(Some(file)) => file,
        Ok(None) => return Err(ApiError::NotFound("New father folder not found".to_string().into()).to_response()),
        Err(_) => return Err(ApiError::InternalServerError(None).to_response()),
    };
    let mut old_father_children = old_father.children;
    let mut new_father_children = new_father.children;
    old_father_children.retain(|x| *x != file._id);
    new_father_children.push(file._id);
    let _ = db
        .update_one(
            doc! {"_id": old_father._id},
            doc! { "$set": doc! { "children": old_father_children },
                    "$set": doc! { "updated_at": Utc::now().timestamp() } },  
            None,
        )
        .await;
    let _ = db
        .update_one(
            doc! {"_id": new_father._id},
            doc! { "$set": doc! { "children": new_father_children },
                    "$set": doc! { "updated_at": Utc::now().timestamp() } },
            None,
        )
        .await;
    let _ = db.replace_one(doc!{"_id": ObjectId::from_str(uuid).unwrap()}, new_metadata, None).await;
    Ok(status::NoContent)
}


#[delete("/<uuid>")]
pub async fn delete_metadata(
    uuid: &str,
    user: AuthenticatedUser,
    mongo: &rocket::State<MongoDb>,
    redis: &rocket::State<Redis>,
    config: &rocket::State<MyConfig>,
) -> Result<status::NoContent, CustomResponse> {
    if redis.exists(uuid).await {
        let _ = redis.delete(uuid).await;
        return Ok(status::NoContent);
    }
    let config = config.inner();
    let db = mongo.database.collection::<File>("files");
    let file = db.find_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()}, None).await;
    let file = mongo_error_check(file, "File")?;
    let _ = check_permission(user, &file)?;
    let father = db.find_one(doc! {"_id": file.father}, None).await;
    let father = mongo_error_check(father, "File")?;
    let mut father_children = father.children;
    father_children.retain(|x| x != &file._id);
    let _ = db.update_one(doc! { "_id": father._id }, doc! { "$set": { "children": father_children } }, None).await.unwrap();
    let _ = db.delete_one(doc! {"_id": ObjectId::from_str(uuid).unwrap()}, None).await.unwrap();
    let path = if file.path == "FLAT" {
        format!("{}/flat/{}", config.storage_path, file._id)
    } else {
        format!("{}/{}", config.storage_path, file.path)
    };
    let _ = crate::file::lib::delete_file(&path);
    Ok(status::NoContent)
}

