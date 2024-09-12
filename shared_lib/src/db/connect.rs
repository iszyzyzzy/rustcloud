use mongodb::{
    bson::{doc, oid::ObjectId},
    Client, Database,
};
use redis::AsyncCommands;

use super::models::{File,LoginedDevice};

pub struct MongoDb {
    pub _client: Client,
    pub database: Database,
}

impl MongoDb {
    pub async fn init(db_uri: &str, db_name: &str) -> Self {
        let client = Client::with_uri_str(&db_uri)
            .await
            .expect("Failed to initialize MongoDB client");
        let database = client.database(db_name);

        MongoDb {
            _client: client,
            database,
        }
    }

    pub async fn get_root_id(&self) -> Option<ObjectId> {
        match self.database.collection::<File>("files").find_one(doc! {"name": "root","type": "Root"}).await {
            Ok(Some(file)) => Some(file._id),
            _ => None,
        }
    }
}

pub struct Redis {
    pub client: redis::Client,
    pub connection_manager: redis::aio::ConnectionManager,
}

impl Redis {
    pub async fn init(redis_uri: &str) -> Self {
        let redis_client =
            redis::Client::open(redis_uri).expect("Failed to initialize Redis client");
        //let mut con = redis_client
        //    .get_connection()
        //    .expect("Failed to get Redis connection while initializing");
        //redis::cmd("FLUSHALL").execute(&mut con);
        Redis {
            client: redis_client.clone(),
            connection_manager: redis::aio::ConnectionManager::new(redis_client)
                .await
                .unwrap(),
        }
    }
    pub async fn recover_from_db(&self, mongodb: &MongoDb) {
        //恢复登录
        let collection = mongodb.database.collection::<LoginedDevice>("logined_devices");
        let mut cursor = collection.find(doc! {}).await.unwrap();
        while cursor.advance().await.unwrap() {
            let device = cursor.deserialize_current().unwrap();
            self.set(&device.uuid, &device.user_uuid.to_hex()).await;
            let _ = self.expire_at(&device.uuid, device.expire_at).await;
            }
    }
    pub async fn get_connection(&self) -> redis::aio::ConnectionManager {
        self.connection_manager.clone()
    }
    pub async fn _queue_push<'a, K, V>(&self, key: K, value: V)
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
        V: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let _: () = con.lpush(key, value).await.unwrap();
    }
    pub async fn _queue_pop<'a, K, RV>(&self, key: K) -> RV
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
        RV: redis::FromRedisValue,
    {
        let mut con = self.get_connection().await;
        
        con.brpop(key, 0.0).await.unwrap()
    }
    pub async fn exists<'a, K>(&self, key: K) -> bool
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let r: bool = con.exists(key).await.unwrap();
        r
    }
    pub async fn _exists_in_range<'a, K, V>(&self, key: K, value: V) -> bool
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
        V: redis::FromRedisValue + Eq + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let values: Vec<V> = con.lrange(key, 0, -1).await.unwrap();
        values.contains(&value)
    }
    pub async fn set<'a, K, V>(&self, key: K, value: V)
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
        V: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let _: () = con.set(key, value).await.unwrap();
    }
    pub async fn get<'a, K, RV>(&self, key: K) -> RV
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
        RV: redis::FromRedisValue,
    {
        let mut con = self.get_connection().await;
        con.get(key).await.unwrap()
    }
    pub async fn delete<'a, K>(&self, key: K)
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let _: () = con.del(key).await.unwrap();
    }
    pub async fn expire<'a, K>(&self, key: K, seconds: i64)
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let _: () = con.expire(key, seconds).await.unwrap();
    }
    pub async fn expire_at<'a, K>(&self, key: K, time: chrono::DateTime<chrono::Utc>) -> Result<(), &str>
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let seconds = time.timestamp() - chrono::Utc::now().timestamp();
        if seconds < 0 {
            return Err("Time is in the past");
        }
        let _: () = con.expire(key, seconds).await.unwrap();
        Ok(())
    }
    pub async fn decr<'a, K>(&self, key: K)
    where
        K: redis::ToRedisArgs + Send + Sync + 'a,
    {
        let mut con = self.get_connection().await;
        let _: () = con.decr(key, 1).await.unwrap();
    }
}
