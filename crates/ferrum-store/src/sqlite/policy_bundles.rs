use ferrum_proto::{PolicyBundle, PolicyBundleId};
use sqlx::Row;
use sqlx::sqlite::{SqlitePool, SqliteRow};

use crate::{PolicyBundleRepo, Result};

pub struct SqlitePolicyBundleRepo {
    pool: SqlitePool,
}

impl SqlitePolicyBundleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_bundle(row: &SqliteRow) -> std::result::Result<PolicyBundle, sqlx::Error> {
        let bundle_id_str: String = row.try_get("bundle_id")?;
        let bundle_id = bundle_id_str
            .parse()
            .map_err(|e: uuid::Error| sqlx::Error::Protocol(e.to_string()))?;
        let name: String = row.try_get("name")?;
        let description: String = row.try_get("description")?;
        let version: String = row.try_get("version")?;
        let created_at: ferrum_proto::Timestamp = row.try_get("created_at")?;
        let updated_at: ferrum_proto::Timestamp = row.try_get("updated_at")?;
        Ok(PolicyBundle {
            bundle_id,
            name,
            description,
            version,
            created_at,
            updated_at,
        })
    }
}

#[async_trait::async_trait]
impl PolicyBundleRepo for SqlitePolicyBundleRepo {
    async fn upsert(&self, bundle: &PolicyBundle) -> Result<()> {
        // H1.1b: Preserve created_at when updating existing bundles.
        // We fetch existing created_at first, then upsert with preserved value.
        let created_at_to_use = {
            let row = sqlx::query("SELECT created_at FROM policy_bundles WHERE bundle_id = ?1")
                .bind(bundle.bundle_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
            match row {
                Some(r) => {
                    let ts: chrono::DateTime<chrono::Utc> =
                        r.try_get("created_at").map_err(crate::StoreError::from)?;
                    ts
                }
                None => bundle.created_at,
            }
        };

        sqlx::query(
            r#"
            INSERT INTO policy_bundles (bundle_id, name, description, version, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(bundle_id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                version = excluded.version,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(bundle.bundle_id.to_string())
        .bind(&bundle.name)
        .bind(&bundle.description)
        .bind(&bundle.version)
        .bind(created_at_to_use)
        .bind(bundle.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, bundle_id: PolicyBundleId) -> Result<Option<PolicyBundle>> {
        let row = sqlx::query("SELECT * FROM policy_bundles WHERE bundle_id = ?1")
            .bind(bundle_id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        Ok(match row {
            Some(r) => Some(Self::row_to_bundle(&r)?),
            None => None,
        })
    }

    async fn list_cursor(
        &self,
        limit: u32,
        after_cursor: Option<&str>,
    ) -> Result<(Vec<PolicyBundle>, Option<String>)> {
        let rows = if let Some(cursor) = after_cursor {
            // Cursor is the created_at of the last item
            sqlx::query(
                r#"
                SELECT * FROM policy_bundles
                WHERE created_at < ?1
                ORDER BY created_at DESC
                LIMIT ?2
                "#,
            )
            .bind(cursor)
            .bind(limit as i64 + 1)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT * FROM policy_bundles
                ORDER BY created_at DESC
                LIMIT ?1
                "#,
            )
            .bind(limit as i64 + 1)
            .fetch_all(&self.pool)
            .await?
        };

        let has_more = rows.len() > limit as usize;
        let items: Vec<PolicyBundle> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| Self::row_to_bundle(&r).map_err(crate::StoreError::from))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let next_cursor = if has_more {
            items.last().map(|b| b.created_at.to_string())
        } else {
            None
        };

        Ok((items, next_cursor))
    }

    async fn update_metadata(
        &self,
        bundle_id: PolicyBundleId,
        name: &str,
        description: &str,
        version: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now();
        let rows_affected = sqlx::query(
            r#"
            UPDATE policy_bundles
            SET name = ?1, description = ?2, version = ?3, updated_at = ?4
            WHERE bundle_id = ?5
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(version)
        .bind(now.to_rfc3339())
        .bind(bundle_id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(crate::StoreError::not_found(
                "policy_bundle",
                bundle_id.to_string(),
            ));
        }
        Ok(())
    }

    async fn delete(&self, bundle_id: PolicyBundleId) -> Result<()> {
        let rows_affected = sqlx::query("DELETE FROM policy_bundles WHERE bundle_id = ?1")
            .bind(bundle_id.to_string())
            .execute(&self.pool)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            return Err(crate::StoreError::not_found(
                "policy_bundle",
                bundle_id.to_string(),
            ));
        }
        Ok(())
    }
}
