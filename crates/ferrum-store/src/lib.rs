pub mod error;
pub mod repos;
pub mod sqlite;
pub mod sync_service;
pub mod transitions;

// PostgreSQL P2 skeleton — compile-time infrastructure only, NOT runtime supported
#[cfg(feature = "postgres")]
pub mod postgres;

pub use error::{Result, StoreError};
pub use repos::StoreFacade;
pub use repos::*;
pub use sqlite::{SqliteStore, SqliteWalTuning};
pub use sync_service::{
    SyncReadinessError, SyncReadinessVerdict, evaluate_sync_readiness_from_cache,
};
