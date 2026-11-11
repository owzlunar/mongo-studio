#[derive(Debug, Clone, Default)]
pub struct QueryProjection {
    pub raw: String,
}

impl QueryProjection {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}
