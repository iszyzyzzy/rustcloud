use mongodb::{
    bson::{doc, oid::ObjectId},
    Client, Database,
};
use redis::AsyncCommands;

use super::models::{File, LoginedDevice};

pub struct MongoDb {
    pub _client: Client,
    pub database: Database,
}

impl MongoDb {
    pub async fn init(db_uri: &str, db_name: &str) -> Self {
        let client = Client::with_uri_str(&db_uri)
            .await
            .expect("Failed to initialize MongoDB client");
        let database = client.database(&db_name);

        MongoDb {
            _client: client,
            database,
        }
    }
    pub async fn first_init(&self) -> Result<ObjectId, ()> {
        let collection_list = self.database.list_collection_names(None).await.unwrap();
        let check_collection = vec![
            "users".to_string(),
            "files".to_string(),
            "logined_devices".to_string(),
        ];
        if compare_vec(&check_collection, &collection_list) {
            print!("载入已有数据...");
            return Ok(self
                .database
                .collection::<File>("files")
                .find_one(doc! {"name": "root","type": "Root"}, None)
                .await
                .unwrap()
                .unwrap()
                ._id);
        }
        if !collection_list.is_empty() {
            print!("警告：数据库非空，已有数据将被清空");
            self.database.drop(None).await.unwrap();
        }
        print!("空数据库,正在初始化...");
        let _ = self
            .database
            .create_collection("users", None)
            .await
            .expect("Failed to create collection");
        let _ = self
            .database
            .create_collection("files", None)
            .await
            .expect("Failed to create collection");
        let _ = self
            .database
            .create_collection("logined_devices", None)
            .await
            .expect("Failed to create collection");

        let metadata_collection = self.database.collection::<File>("files");
        let root_id = ObjectId::new();
        let root = File {
            _id: root_id.clone(),
            name: "root".to_string(),
            type_: super::models::FileType::Root,
            father: root_id.clone(),
            children: vec![],
            owner: root_id.clone(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            size: 0,
            sha256: "".to_string(),
            path: "FLAT".to_string(),
        };
        let _ = metadata_collection.insert_one(root, None).await;

        let _ = crate::auth::lib::create_user("admin", "admin", "admin", &self, &root_id)
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
        let _ = logined_device_collection
            .create_index(index_model, None)
            .await;
        Ok(root_id)
    }
}

fn compare_vec(a: &Vec<String>, b: &Vec<String>) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut a = a.clone();
    let mut b = b.clone();
    a.sort();
    b.sort();
    a == b
}

pub struct Redis {
    //    pub client: redis::Client,
    pub connection_manager: redis::aio::ConnectionManager,
}

impl Redis {
    pub async fn init(redis_uri: &str) -> Self {
        let redis_client =
            redis::Client::open(redis_uri).expect("Failed to initialize Redis client");
        let mut con = redis_client
            .get_connection()
            .expect("Failed to get Redis connection while initializing");
        redis::cmd("FLUSHALL").execute(&mut con);
        Redis {
            connection_manager: redis::aio::ConnectionManager::new(redis_client)
                .await
                .unwrap(),
        }
    }
    pub async fn get_connection(&self) -> redis::aio::ConnectionManager {
        self.connection_manager.clone()
    }
    pub async fn _queue_push(&self, key: &str, value: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.lpush(key, value).await.unwrap();
    }
    pub async fn _queue_pop(&self, key: &str) -> String {
        let mut con = self.get_connection().await;
        let value: String = con.brpop(key, 0.0).await.unwrap();
        value
    }
    pub async fn exists(&self, key: &str) -> bool {
        let mut con = self.get_connection().await;
        let r: bool = con.exists(key).await.unwrap();
        r
    }
    pub async fn _exists_in_range(&self, key: &str, value: &str) -> bool {
        let mut con = self.get_connection().await;
        let values: Vec<String> = con.lrange(key, 0, -1).await.unwrap();
        values.contains(&value.to_string())
    }
    pub async fn set(&self, key: &str, value: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.set(key, value).await.unwrap();
    }
    pub async fn get(&self, key: &str) -> Result<String, redis::RedisError> {
        let mut con = self.get_connection().await;
        con.get(key).await
    }
    pub async fn delete(&self, key: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.del(key).await.unwrap();
    }
    pub async fn expire(&self, key: &str, seconds: i64) {
        let mut con = self.get_connection().await;
        let _: () = con.expire(key, seconds).await.unwrap();
    }
}
