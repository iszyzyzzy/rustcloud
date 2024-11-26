//用来存放与ref相关的操作
//虽然不是storage_backend的一部分，但是也放在这里

use crate::db::models::File;
use crate::libs::ApiError;
use mongodb::bson::doc;

pub async fn find_existed_with_sha256(
    collection: &mongodb::Collection<File>,
    sha256: &str,
) -> Result<Option<File>, ApiError> {
    let metadata = collection
        .find_one(doc! {
            "sha256": sha256,
            "storage_type": doc! {"$ne": "ref"}
        })
        .await;
    match metadata {
        Ok(Some(metadata)) => Ok(Some(metadata)),
        _ => Ok(None),
    }
}

pub async fn find_and_add_ref(collection: &mongodb::Collection<File>, file: &File) -> Result<Option<File>, ApiError> {
    if let Some(exist) = find_existed_with_sha256(collection, &file.sha256.clone()).await? {
        let _ = collection
            .update_one(
                doc! {"_id": exist._id.clone()},
                doc! {"$push": {"file_references": file._id.clone()}},
            )
            .await;
        return Ok(Some(exist));
    }
    Ok(None)
}

pub async fn remove_ref(collection: &mongodb::Collection<File>, file: &File) -> Result<(), ApiError> {
    let _ = collection
        .update_one(
            doc! {"_id": file.path.clone()},
            doc! {"$pull": {"file_references": file._id.clone()}},
        )
        .await;
    Ok(())
}

pub async fn change_mother(collection: &mongodb::Collection<File>, file: &File) -> Result<(), ApiError> {
    if let Some(ext) = &file.extra_metadata {
        //选一个new_mother出来
        let mut left_list = ext.file_references.clone();
        let new_mother = left_list.pop().unwrap();
        let new_mother_extra_metadata = collection
            .find_one(doc! {"_id": new_mother})
            .await
            .unwrap()
            .unwrap()
            .extra_metadata
            .unwrap_or_default();
        let new_mother_extra_metadata = shared_lib::db::models::FileExtraMetadata {
            file_references: left_list.clone(),
            ..new_mother_extra_metadata
        };
        let _ = collection
            .update_one(
                doc! { "_id": new_mother },
                doc! {
                    "$set": {
                    "extra_metadata": new_mother_extra_metadata,
                    "storage_type": "FLAT",
                    "path": file.path.clone()
                } },
            )
            .await
            .unwrap();
        /*                             let _ = db.update_many(
            doc! { "_id": { "$in": list } },
            doc! { "$set": { "path": new_mother } },
        ); */

        for id in left_list {
            let _ = collection
                .update_one(
                    doc! { "_id": id },
                    doc! { "$set": { "path": new_mother.to_hex() } },
                )
                .await;
        };
    }
    Ok(())
}