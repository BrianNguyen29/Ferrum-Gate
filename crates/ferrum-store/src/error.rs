use thiserror::Error;

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("{entity} '{id}' was not found")]
    NotFound { entity: &'static str, id: String },
    #[error("invalid state transition: {0}")]
    InvalidState(String),
    #[error(
        "database schema version {db_version} is newer than the supported version {expected_version}; upgrade the binary before starting"
    )]
    SchemaDrift {
        db_version: i64,
        expected_version: i64,
    },
    #[error("{0}")]
    Other(String),
}

impl StoreError {
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }
}
