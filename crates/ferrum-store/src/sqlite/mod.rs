mod approvals;
mod capabilities;
mod executions;
mod helpers;
mod intents;
mod ledger;
mod migrations;
mod proposals;
mod provenance;
mod rollback;

#[cfg(test)]
mod tests;

pub use approvals::SqliteApprovalRepo;
pub use capabilities::SqliteCapabilityRepo;
pub use executions::SqliteExecutionRepo;
pub use intents::SqliteIntentRepo;
pub use ledger::SqliteLedgerRepo;
pub use proposals::SqliteProposalRepo;
pub use provenance::SqliteProvenanceRepo;
pub use rollback::SqliteRollbackRepo;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use crate::Result;

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn apply_embedded_migrations(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let mut statement = String::new();

        for line in migrations::INIT_MIGRATION.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("--") {
                continue;
            }

            statement.push_str(line);
            statement.push('\n');

            if trimmed.ends_with(';') {
                let sql = statement.trim();
                if !sql.is_empty() {
                    sqlx::query(sql).execute(&mut *tx).await?;
                }
                statement.clear();
            }
        }

        tx.commit().await?;
        Ok(())
    }

    pub fn intents(&self) -> SqliteIntentRepo {
        SqliteIntentRepo::new(self.pool.clone())
    }

    pub fn proposals(&self) -> SqliteProposalRepo {
        SqliteProposalRepo::new(self.pool.clone())
    }

    pub fn capabilities(&self) -> SqliteCapabilityRepo {
        SqliteCapabilityRepo::new(self.pool.clone())
    }

    pub fn executions(&self) -> SqliteExecutionRepo {
        SqliteExecutionRepo::new(self.pool.clone())
    }

    pub fn rollback_contracts(&self) -> SqliteRollbackRepo {
        SqliteRollbackRepo::new(self.pool.clone())
    }

    pub fn approvals(&self) -> SqliteApprovalRepo {
        SqliteApprovalRepo::new(self.pool.clone())
    }

    pub fn provenance(&self) -> SqliteProvenanceRepo {
        SqliteProvenanceRepo::new(self.pool.clone())
    }

    pub fn ledger(&self) -> SqliteLedgerRepo {
        SqliteLedgerRepo::new(self.pool.clone())
    }
}
