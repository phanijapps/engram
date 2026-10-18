//! Host-facing model selection, independent of storage and model runtimes.
mod identity;
mod overrides;
mod selection;

pub use overrides::EmbeddingOverrides;
use serde::{Deserialize, Serialize};

pub const MAX_EMBEDDING_DIMENSIONS: u32 = 8192;

/// Provider-specific runtime options. Secret values are never configuration fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_instruction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passage_instruction: Option<String>,
    pub request_timeout_secs: u64,
    pub batch_size: usize,
}

impl Default for EmbeddingOptions {
    fn default() -> Self {
        Self {
            endpoint: None,
            api_key_env: None,
            query_instruction: None,
            passage_instruction: None,
            request_timeout_secs: 30,
            batch_size: 32,
        }
    }
}

/// Existing v1 host settings plus additive, flattened provider options.
/// `dimensions == 0` denotes automatic inference until provider construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EmbeddingProviderConfig {
    pub provider_type: String,
    pub model: String,
    #[serde(skip_serializing_if = "is_automatic")]
    pub dimensions: u32,
    pub prompt_profile: String,
    pub normalization: Option<String>,
    #[serde(flatten)]
    pub options: EmbeddingOptions,
}

impl Default for EmbeddingProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: "fastembed".into(),
            model: "BAAI/bge-small-en-v1.5".into(),
            dimensions: 384,
            prompt_profile: "query".into(),
            normalization: None,
            options: Default::default(),
        }
    }
}

impl EmbeddingProviderConfig {
    /// Checks portable settings; model-runtime validation happens before index construction.
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() || self.provider_type.trim().is_empty() {
            return Err("embedding provider and model must be nonempty".into());
        }
        if self.dimensions > MAX_EMBEDDING_DIMENSIONS {
            return Err("embedding dimensions must not exceed 8192".into());
        }
        if !(1..=300).contains(&self.options.request_timeout_secs)
            || !(1..=256).contains(&self.options.batch_size)
        {
            return Err("embedding timeout must be 1..300 seconds and batch_size 1..256".into());
        }
        if self.prompt_profile.trim().is_empty() {
            return Err("embedding prompt_profile must be nonempty".into());
        }
        if !matches!(self.normalization.as_deref(), None | Some("none" | "l2")) {
            return Err("embedding normalization must be none or l2".into());
        }
        for text in [
            &self.options.query_instruction,
            &self.options.passage_instruction,
        ]
        .into_iter()
        .flatten()
        {
            if text.len() > 8192 {
                return Err("embedding instruction exceeds 8192 bytes".into());
            }
        }
        if self.provider_type == "fastembed"
            && (self.options.endpoint.is_some() || self.options.api_key_env.is_some())
        {
            return Err("FastEmbed does not accept an endpoint or API key".into());
        }
        Ok(())
    }
}

fn is_automatic(dimensions: &u32) -> bool {
    *dimensions == 0
}
