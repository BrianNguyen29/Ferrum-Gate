use async_trait::async_trait;
use ferrum_proto::{PolicyBundle, PolicyBundleVersion};
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

    /// Insert a version row for the given bundle with the next monotonic version number.
    async fn insert_version(
        &self,
        bundle: &PolicyBundle,
        note: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<i64> {
        let raw_json = to_json(bundle)?;
        let new_version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM policy_bundle_version WHERE bundle_id = ?1",
        )
        .bind(&bundle.bundle_id)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            "INSERT INTO policy_bundle_version (
                id, bundle_id, version, content, active, created_at, created_by, note
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&bundle.bundle_id)
        .bind(new_version)
        .bind(raw_json)
        .bind(bundle.active as i32)
        .bind(bundle.created_at)
        .bind(created_by)
        .bind(note)
        .execute(&self.pool)
        .await?;

        Ok(new_version)
    }

    /// Update the active flag on the latest version row for a bundle.
    async fn update_version_active(&self, bundle_id: &str, active: bool) -> Result<()> {
        sqlx::query(
            "UPDATE policy_bundle_version
             SET active = ?2
             WHERE bundle_id = ?1
               AND version = (SELECT MAX(version) FROM policy_bundle_version WHERE bundle_id = ?1)",
        )
        .bind(bundle_id)
        .bind(active as i32)
        .execute(&self.pool)
        .await?;
        Ok(())
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

        self.insert_version(bundle, Some("Created"), None).await?;
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

        self.insert_version(bundle, Some("Updated"), None).await?;
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
        let Some(mut bundle) = self.get(bundle_id).await? else {
            return Ok(());
        };
        bundle.active = active;
        self.update(&bundle).await?;
        self.update_version_active(bundle_id, active).await?;
        Ok(())
    }

    async fn list_versions(&self, bundle_id: &str) -> Result<Vec<PolicyBundleVersion>> {
        let rows = sqlx::query(
            "SELECT id, bundle_id, version, content, active, created_at, created_by, note
             FROM policy_bundle_version
             WHERE bundle_id = ?1
             ORDER BY version DESC",
        )
        .bind(bundle_id)
        .fetch_all(&self.pool)
        .await?;

        let mut versions = Vec::with_capacity(rows.len());
        for row in rows {
            let content_raw: String = row.try_get("content")?;
            let content: PolicyBundle = from_json(&content_raw)?;
            versions.push(PolicyBundleVersion {
                id: row.try_get("id")?,
                bundle_id: row.try_get("bundle_id")?,
                version: row.try_get("version")?,
                content,
                active: row.try_get::<i32, _>("active")? != 0,
                created_at: row.try_get("created_at")?,
                created_by: row.try_get("created_by")?,
                note: row.try_get("note")?,
            });
        }
        Ok(versions)
    }

    async fn get_version(
        &self,
        bundle_id: &str,
        version: i64,
    ) -> Result<Option<PolicyBundleVersion>> {
        let row = sqlx::query(
            "SELECT id, bundle_id, version, content, active, created_at, created_by, note
             FROM policy_bundle_version
             WHERE bundle_id = ?1 AND version = ?2",
        )
        .bind(bundle_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let content_raw: String = row.try_get("content")?;
                let content: PolicyBundle = from_json(&content_raw)?;
                Ok(Some(PolicyBundleVersion {
                    id: row.try_get("id")?,
                    bundle_id: row.try_get("bundle_id")?,
                    version: row.try_get("version")?,
                    content,
                    active: row.try_get::<i32, _>("active")? != 0,
                    created_at: row.try_get("created_at")?,
                    created_by: row.try_get("created_by")?,
                    note: row.try_get("note")?,
                }))
            }
            None => Ok(None),
        }
    }

    async fn rollback(
        &self,
        bundle_id: &str,
        target_version: i64,
        actor: Option<&str>,
    ) -> Result<i64> {
        let target = self
            .get_version(bundle_id, target_version)
            .await?
            .ok_or_else(|| {
                crate::StoreError::not_found("policy_bundle_version", target_version.to_string())
            })?;

        let mut new_bundle = target.content;
        new_bundle.active = true;
        new_bundle.updated_at = chrono::Utc::now();
        let raw_json = to_json(&new_bundle)?;

        sqlx::query(
            "UPDATE policy_bundles
             SET version = ?2,
                 active = ?3,
                 content_hash = ?4,
                 updated_at = ?5,
                 raw_json = ?6
             WHERE bundle_id = ?1",
        )
        .bind(&new_bundle.bundle_id)
        .bind(&new_bundle.version)
        .bind(new_bundle.active as i32)
        .bind(new_bundle.content_hash.as_deref().unwrap_or(""))
        .bind(new_bundle.updated_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;

        let note = format!("Rollback to v{}", target_version);
        let new_version = self.insert_version(&new_bundle, Some(&note), actor).await?;
        self.update_version_active(bundle_id, true).await?;

        Ok(new_version)
    }
}
