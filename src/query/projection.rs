use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryProjection {
    pub raw: String,
}

impl QueryProjection {
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}

impl AsRef<str> for QueryProjection {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}
