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
        .bind(bundle.created_at)
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
}
