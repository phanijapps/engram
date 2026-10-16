//! Validated internal SQL identifiers. Callers supply a digest, never SQL.
use engram_runtime::{CoreError, CoreResult};

#[derive(Clone)]
pub(super) struct VectorNamespace {
    table: String,
    metadata: String,
    legacy: bool,
}
impl VectorNamespace {
    pub fn legacy() -> Self {
        Self {
            table: "vectors".into(),
            metadata: "vector_meta".into(),
            legacy: true,
        }
    }
    pub fn from_digest(digest: &str) -> CoreResult<Self> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(CoreError::InvalidRequest {
                reason: "vector namespace must be a lowercase SHA-256 hex digest".into(),
            });
        }
        Ok(Self {
            table: format!("vectors_{digest}"),
            metadata: format!("vector_meta_{digest}"),
            legacy: false,
        })
    }
    pub fn is_legacy(&self) -> bool {
        self.legacy
    }
    /// Only adapter-owned SQL literals may be passed here.
    pub fn sql(&self, template: &str) -> String {
        template
            .replace("vectors", &self.table)
            .replace("vector_meta", &self.metadata)
    }
}
