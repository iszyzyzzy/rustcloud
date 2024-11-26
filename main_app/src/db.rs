pub use shared_lib::db::connect;
pub use shared_lib::db::models;

use shared_lib::db::connect::{MongoDb, Redis};
use mongodb::bson::{doc, oid::ObjectId};
use shared_lib::db::models::{File, FileType, LoginedDevice};

pub trait FirstInit {
    async fn first_init(&mut self) -> Result<(),()>;
}

impl FirstInit for MongoDb {
    async fn first_init(&mut self) -> Result<(),()> {
        let collection_list = self.database.list_collection_names().await.unwrap();
        let check_collection = vec![
            "users".to_string(),
            "files".to_string(),
            "logined_devices".to_string(),
        ];
        if compare_vec(&check_collection, &collection_list) {
            print!("载入已有数据...");
            return Ok(());
/*             return Ok(self
                .database
                .collection::<File>("files")
                .find_one(doc! {"name": "root","type": "Root"})
                .await
                .unwrap()
                .unwrap()
                ._id); */
        }
        if !collection_list.is_empty() {
            print!("警告：数据库非空，已有数据将被清空");
            self.database.drop().await.unwrap();
        }
        print!("空数据库,正在初始化...");
        self.database
            .create_collection("users")
            .await
            .expect("Failed to create collection");
        self.database
            .create_collection("files")
            .await
            .expect("Failed to create collection");
        self.database
            .create_collection("logined_devices")
            .await
            .expect("Failed to create collection");

        let metadata_collection = self.database.collection::<File>("files");
        let root_id = ObjectId::new();
        let root = File {
            _id: root_id,
            name: "root".to_string(),
            type_: FileType::Root,
            father: root_id,
            children: vec![],
            owner: root_id,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            size: 0,
            sha256: "".to_string(),
            path: root_id.to_hex(),
            storage_type: "FLAT".to_string(),
            extra_metadata: None,
        };
        let _ = metadata_collection.insert_one(root).await;

        crate::auth::lib::create_user("admin", "admin", "admin", self, &root_id)
            .await
            .unwrap();

        let logined_device_collection =
            self.database.collection::<LoginedDevice>("logined_devices");
        let index_model: mongodb::IndexModel = mongodb::IndexModel::builder()
            .keys(doc! { "expire_at": 1 })
            .options(
                mongodb::options::IndexOptions::builder()
                    .expire_after(std::time::Duration::from_secs(1))
                    .build(),
            )
            .build();
        let _ = logined_device_collection.create_index(index_model).await;
        Ok(())
    }
}

impl FirstInit for Redis {
    async fn first_init(&mut self) -> Result<(),()> {
            redis::cmd("FLUSHALL").query::<()>(&mut self.client).unwrap();
            Ok(())
        }
}

fn compare_vec(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut a = a.to_owned();
    let mut b = b.to_owned();
    a.sort();
    b.sort();
    a == b
}
