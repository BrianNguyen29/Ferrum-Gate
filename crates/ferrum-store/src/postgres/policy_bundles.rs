//! PostgreSQL PolicyBundleRepo implementation (P3 runtime slice).

use async_trait::async_trait;
use ferrum_proto::PolicyBundle;
use sqlx::{PgPool, Row};

use crate::{PolicyBundleRepo, Result};

use super::helpers::{fetch_entities, fetch_entity_by_id, from_json, to_json};

#[derive(Clone)]
pub struct PostgresPolicyBundleRepo {
    pool: PgPool,
}

impl PostgresPolicyBundleRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyBundleRepo for PostgresPolicyBundleRepo {
    async fn insert(&self, bundle: &PolicyBundle) -> Result<()> {
        let raw_json = to_json(bundle)?;
        sqlx::query(
            "INSERT INTO policy_bundles (
                bundle_id, version, active, content_hash, created_at, updated_at, raw_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.version)
        .bind(bundle.active)
        .bind(bundle.content_hash.as_deref().unwrap_or(""))
        .bind(bundle.created_at.to_rfc3339())
        .bind(bundle.updated_at.to_rfc3339())
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, bundle_id: &str) -> Result<Option<PolicyBundle>> {
        fetch_entity_by_id(&self.pool, "policy_bundles", "bundle_id", bundle_id).await
    }

    async fn get_by_content_hash(&self, content_hash: &str) -> Result<Option<PolicyBundle>> {
        let row = sqlx::query("SELECT raw_json FROM policy_bundles WHERE content_hash = $1")
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
             SET version = $2,
                 active = $3,
                 content_hash = $4,
                 updated_at = $5,
                 raw_json = $6
             WHERE bundle_id = $1",
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.version)
        .bind(bundle.active)
        .bind(bundle.content_hash.as_deref().unwrap_or(""))
        .bind(bundle.updated_at.to_rfc3339())
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, bundle_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM policy_bundles WHERE bundle_id = $1")
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
            "SELECT raw_json FROM policy_bundles WHERE active = true ORDER BY created_at DESC",
            |query| query,
        )
        .await
    }

    async fn set_active(&self, bundle_id: &str, active: bool) -> Result<()> {
        let mut bundle = match self.get(bundle_id).await? {
            Some(b) => b,
            None => return Ok(()),
        };
        bundle.active = active;
        bundle.updated_at = chrono::Utc::now();
        let raw_json = to_json(&bundle)?;
        sqlx::query(
            "UPDATE policy_bundles
             SET active = $2,
                 updated_at = $3,
                 raw_json = $4
             WHERE bundle_id = $1",
        )
        .bind(bundle_id)
        .bind(active)
        .bind(bundle.updated_at.to_rfc3339())
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
