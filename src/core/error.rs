use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Invalid JSON query filter: {0}")]
    InvalidQueryJson(String),

    #[error("Collection not found: {0}.{1}")]
    CollectionNotFound(String, String),

    #[error("Persistence error: {0}")]
    Persistence(String),
}

pub type AppResult<T> = Result<T, AppError>;
