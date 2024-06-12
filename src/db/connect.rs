use mongodb::{Client, Database};
use redis::AsyncCommands;
use super::models::User;

pub struct MongoDb {
    pub client: Client,
    pub database: Database,
}

impl MongoDb {
    pub async fn init(db_uri: &str, db_name: &str) -> Self {
        let client = Client::with_uri_str(&db_uri)
            .await
            .expect("Failed to initialize MongoDB client");
        let database = client.database(&db_name);

        MongoDb { client, database }
    }
    pub async fn first_init(&self) {
        let collection_list = self.database.list_collection_names(None).await.unwrap();
        let check_collection = vec!["users".to_string(), "metadata".to_string()];
        if compare_vec(&check_collection, &collection_list) {
            return;
        }
        if collection_list.is_empty() {
            let _ = self
                .database
                .create_collection("users", None)
                .await
                .expect("Failed to create collection");
            let _ = self
                .database
                .create_collection("metadata", None)
                .await
                .expect("Failed to create collection");

            let _ = crate::auth::lib::create_user("admin", "admin", "admin", &self).await;

            //let metadata_collection = self.database.collection::<Metadata>("metadata");
            //TODO
                
        }
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
    pub async fn init(redis_uri: &str,) -> Self {
        let redis_client = redis::Client::open(redis_uri).expect("Failed to initialize Redis client");
        Redis { connection_manager: redis::aio::ConnectionManager::new(redis_client).await.unwrap() }
    }
    pub async fn get_connection(&self) -> redis::aio::ConnectionManager {
        self.connection_manager.clone()
    }
    pub async fn queue_push(&self, key: &str, value: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.lpush(key, value).await.unwrap();
    }
    pub async fn queue_pop(&self, key: &str) -> String {
        let mut con = self.get_connection().await;
        let value: String = con.brpop(key, 0.0).await.unwrap();
        value
    }
    pub async fn exists(&self, key: &str) -> bool {
        let mut con = self.get_connection().await;
        let _: () = con.exists(key).await.unwrap();
        true
    }
    pub async fn delete(&self, key: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.del(key).await.unwrap();
    }
    pub async fn set(&self, key: &str, value: &str) {
        let mut con = self.get_connection().await;
        let _: () = con.set(key, value).await.unwrap();
    }
}