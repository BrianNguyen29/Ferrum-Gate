use async_trait::async_trait;
use ferrum_proto::PolicyBundle;
use sqlx::{Row, SqlitePool};

use crate::sqlite::write_queue::WriteQueue;
use crate::{PolicyBundleRepo, Result};

use super::helpers::{fetch_entities, fetch_entity_by_id, from_json, to_json};

#[derive(Clone)]
pub struct SqlitePolicyBundleRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqlitePolicyBundleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            write_queue: None,
        }
    }

    pub fn with_write_queue(mut self, queue: WriteQueue) -> Self {
        self.write_queue = Some(queue);
        self
    }
}

#[async_trait]
impl PolicyBundleRepo for SqlitePolicyBundleRepo {
    async fn insert(&self, bundle: &PolicyBundle) -> Result<()> {
        let raw_json = to_json(bundle)?;
        sqlx::query(
            "INSERT INTO policy_bundles (
                bundle_id, version, active, content_hash, created_at, updated_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.version)
        .bind(bundle.active as i32)
        .bind(bundle.content_hash.as_deref().unwrap_or(""))
        .bind(bundle.created_at)
        .bind(bundle.updated_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, bundle_id: &str) -> Result<Option<PolicyBundle>> {
        fetch_entity_by_id(&self.pool, "policy_bundles", "bundle_id", bundle_id).await
    }

    async fn get_by_content_hash(&self, content_hash: &str) -> Result<Option<PolicyBundle>> {
        let row = sqlx::query("SELECT raw_json FROM policy_bundles WHERE content_hash = ?1")
            .bind(content_hash)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => {
                let raw: String = row.try_get("raw_json")?;
                Ok(Some(from_json(&raw)?))
            }
            None => Ok(None),
        }
    }

    async fn update(&self, bundle: &PolicyBundle) -> Result<()> {
        let raw_json = to_json(bundle)?;
        sqlx::query(
            "UPDATE policy_bundles
             SET version = ?2,
                 active = ?3,
                 content_hash = ?4,
                 updated_at = ?5,
                 raw_json = ?6
             WHERE bundle_id = ?1",
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.version)
        .bind(bundle.active as i32)
        .bind(bundle.content_hash.as_deref().unwrap_or(""))
        .bind(bundle.updated_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, bundle_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM policy_bundles WHERE bundle_id = ?1")
            .bind(bundle_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<PolicyBundle>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM policy_bundles ORDER BY created_at DESC",
            |query| query,
        )
        .await
    }

    async fn list_active(&self) -> Result<Vec<PolicyBundle>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM policy_bundles WHERE active = 1 ORDER BY created_at DESC",
            |query| query,
        )
        .await
    }

    async fn set_active(&self, bundle_id: &str, active: bool) -> Result<()> {
        sqlx::query("UPDATE policy_bundles SET active = ?2 WHERE bundle_id = ?1")
            .bind(bundle_id)
            .bind(active as i32)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
