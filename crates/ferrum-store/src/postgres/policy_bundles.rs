//! PostgreSQL PolicyBundleRepo skeleton (P2 placeholder).
//!
//! **NOT runtime supported.** All operations return P2 skeleton error.

use async_trait::async_trait;
use ferrum_proto::PolicyBundle;
use sqlx::PgPool;

use super::skeleton_error;
use crate::{PolicyBundleRepo, Result};

#[derive(Debug, Clone)]
pub struct PostgresPolicyBundleRepo {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresPolicyBundleRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PolicyBundleRepo for PostgresPolicyBundleRepo {
    async fn insert(&self, _bundle: &PolicyBundle) -> Result<()> {
        Err(skeleton_error())
    }

    async fn get(&self, _bundle_id: &str) -> Result<Option<PolicyBundle>> {
        Err(skeleton_error())
    }

    async fn get_by_content_hash(&self, _content_hash: &str) -> Result<Option<PolicyBundle>> {
        Err(skeleton_error())
    }

    async fn update(&self, _bundle: &PolicyBundle) -> Result<()> {
        Err(skeleton_error())
    }

    async fn delete(&self, _bundle_id: &str) -> Result<()> {
        Err(skeleton_error())
    }

    async fn list(&self) -> Result<Vec<PolicyBundle>> {
        Err(skeleton_error())
    }

    async fn list_active(&self) -> Result<Vec<PolicyBundle>> {
        Err(skeleton_error())
    }

    async fn set_active(&self, _bundle_id: &str, _active: bool) -> Result<()> {
        Err(skeleton_error())
    }
}
