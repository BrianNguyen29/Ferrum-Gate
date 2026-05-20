use async_trait::async_trait;
use ferrum_proto::ScopedToken;
use sqlx::PgPool;

use crate::{Result, TokenRepo};

/// PostgreSQL token repo — compile-time skeleton for token storage.
///
/// **Not yet fully implemented.** All methods return `StoreError::Other`
/// with a descriptive message. This exists so that `PostgresStore` can
/// implement `StoreFacade::tokens()` without breaking the build.
#[derive(Clone)]
pub struct PostgresTokenRepo {
    _pool: PgPool,
}

impl PostgresTokenRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { _pool: pool }
    }
}

#[async_trait]
impl TokenRepo for PostgresTokenRepo {
    async fn insert(&self, _token: &ScopedToken) -> Result<()> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::insert not yet implemented".to_string(),
        ))
    }

    async fn get(&self, _token_id: &str) -> Result<Option<ScopedToken>> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::get not yet implemented".to_string(),
        ))
    }

    async fn get_by_lookup_hash(&self, _lookup_hash: &str) -> Result<Option<ScopedToken>> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::get_by_lookup_hash not yet implemented".to_string(),
        ))
    }

    async fn list(
        &self,
        _actor_id: Option<&str>,
        _role: Option<&str>,
        _active_only: bool,
        _limit: u32,
        _cursor: Option<&str>,
    ) -> Result<(Vec<ScopedToken>, Option<String>)> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::list not yet implemented".to_string(),
        ))
    }

    async fn revoke(&self, _token_id: &str, _reason: Option<&str>) -> Result<bool> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::revoke not yet implemented".to_string(),
        ))
    }

    async fn touch(&self, _token_id: &str) -> Result<()> {
        Err(crate::StoreError::Other(
            "PostgresTokenRepo::touch not yet implemented".to_string(),
        ))
    }
}
