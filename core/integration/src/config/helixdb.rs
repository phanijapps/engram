//! Portable storage selection. No engine or credential values are held here.
use engram_domain::EmbeddingSpace;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageProvider {
    Sqlite,
    Pgvector,
    Helixdb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HelixMode {
    Service,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HelixConfig {
    pub mode: Option<HelixMode>,
    pub logical_store: String,
    pub endpoint: Option<String>,
    pub credential_env: Option<String>,
    pub path: Option<PathBuf>,
    pub read_only: bool,
    pub embedding_space: Option<EmbeddingSpace>,
    pub queue_capacity: usize,
    pub timeout_millis: u64,
}
impl Default for HelixConfig {
    fn default() -> Self {
        Self {
            mode: None,
            logical_store: String::new(),
            endpoint: None,
            credential_env: None,
            path: None,
            read_only: false,
            embedding_space: None,
            queue_capacity: 8,
            timeout_millis: 120_000,
        }
    }
}
