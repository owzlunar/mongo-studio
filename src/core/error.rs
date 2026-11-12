use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("connection error: {0}")]
    Connection(String),

    #[error("invalid JSON query filter: {0}")]
    InvalidQueryJson(String),

    #[error("collection not found: {0}.{1}")]
    CollectionNotFound(String, String),

    #[error("persistence error: {0}")]
    Persistence(String),
}

pub type AppResult<T> = Result<T, AppError>;
