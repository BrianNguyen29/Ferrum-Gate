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
    #[error("internal error: {0}")]
    Internal(String),
    /// H1.1c: Constraint violation — e.g., referential integrity blocked delete.
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl StoreError {
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }

    /// H1.1c: Create a constraint violation error (e.g., for blocked deletes).
    pub fn constraint_violation(msg: impl Into<String>) -> Self {
        Self::ConstraintViolation(msg.into())
    }
}
