use serde::{Deserialize, Serialize};
use sqlx::PgPool;

pub mod watchdog;

#[derive(Debug, Clone)]
pub struct ArchiveConnector {
    pool: PgPool,
}

impl ArchiveConnector {
    pub async fn connect() -> Self {
        // TODO(adonagy): unwrap
        let db_url = if let Ok(url) = dotenvy::var("DATABASE_URL") {
            url
        } else {
            std::env::var("DATABASE_URL").expect("No db url found, check env var DATABASE_URL")
        };

        let pool = PgPool::connect(&db_url).await.unwrap();

        Self { pool }
    }

    pub async fn get_producer_blocks(&self, producer_pk: &str) -> Result<Vec<Block>, sqlx::Error> {
        sqlx::query_as!(
            Block,
            r#"SELECT 
                    b.id, 
                    b.state_hash, 
                    b.height, 
                    b.timestamp, 
                    b.chain_status AS "chain_status: ChainStatus",
                    pk_creator.value AS "creator_key",
                    pk_winner.value AS "winner_key",
                    b.global_slot_since_genesis,
                    b.global_slot_since_hard_fork
                FROM 
                    blocks b
                JOIN 
                    public_keys pk_creator ON b.creator_id = pk_creator.id
                JOIN 
                    public_keys pk_winner ON b.block_winner_id = pk_winner.id
                WHERE 
                    pk_creator.value = $1"#,
            producer_pk
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_blocks_in_slot_range(
        &self,
        start_slot: i64,
        finish_slot: i64,
    ) -> Result<Vec<Block>, sqlx::Error> {
        sqlx::query_as!(
            Block,
            r#"SELECT 
                    b.id, 
                    b.state_hash, 
                    b.height, 
                    b.timestamp, 
                    b.chain_status AS "chain_status: ChainStatus",
                    pk_creator.value AS "creator_key",
                    pk_winner.value AS "winner_key",
                    b.global_slot_since_genesis,
                    b.global_slot_since_hard_fork
                FROM 
                    blocks b
                JOIN 
                    public_keys pk_creator ON b.creator_id = pk_creator.id
                JOIN 
                    public_keys pk_winner ON b.block_winner_id = pk_winner.id
                WHERE 
                    b.global_slot_since_hard_fork BETWEEN $1 AND $2"#,
            start_slot,
            finish_slot
        )
        .fetch_all(&self.pool)
        .await
    }
}

#[derive(sqlx::Type, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[sqlx(type_name = "chain_status_type", rename_all = "lowercase")]
pub enum ChainStatus {
    Canonical,
    Orphaned,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    id: i32,
    pub state_hash: String,
    pub height: i64,
    timestamp: String,
    pub chain_status: ChainStatus,
    pub creator_key: String,
    winner_key: String,
    global_slot_since_hard_fork: i64,
    global_slot_since_genesis: i64,
}

impl Block {
    pub fn global_slot(&self) -> u32 {
        self.global_slot_since_hard_fork as u32
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test() {
        let db = ArchiveConnector::connect().await;

        let blocks = db
            .get_producer_blocks("B62qkPpK6z4ktWjxcmFzM4cFWjWLzrjNh6USjUMiYGcF3YAVbdo2p4H")
            .await
            .unwrap();

        let canonical = blocks
            .iter()
            .filter(|block| block.chain_status == ChainStatus::Pending)
            .collect::<Vec<_>>();

        println!("Canonical blocks: {}", canonical.len());
    }
}
