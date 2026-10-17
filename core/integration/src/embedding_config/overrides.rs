use super::EmbeddingProviderConfig;
use serde::{Deserialize, Serialize};

/// Explicit host overrides. None means inherit, including an omitted dimension.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EmbeddingOverrides {
    pub preset: Option<String>,
    pub provider_type: Option<String>,
    pub model: Option<String>,
    pub dimensions: Option<u32>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    pub query_instruction: Option<String>,
    pub passage_instruction: Option<String>,
    pub request_timeout_secs: Option<u64>,
    pub batch_size: Option<usize>,
}

impl EmbeddingOverrides {
    pub(crate) fn apply(self, config: &mut EmbeddingProviderConfig) -> Result<(), String> {
        if let Some(preset) = self.preset {
            *config = EmbeddingProviderConfig::preset(&preset)?;
        }
        if let Some(provider) = self.provider_type {
            if provider != config.provider_type && self.dimensions.is_none() {
                config.dimensions = 0;
            }
            config.provider_type = provider;
        }
        if let Some(model) = self.model {
            if model != config.model && self.dimensions.is_none() {
                config.dimensions = 0;
            }
            config.model = model;
        }
        if let Some(dimensions) = self.dimensions {
            if !(1..=super::MAX_EMBEDDING_DIMENSIONS).contains(&dimensions) {
                return Err("explicit embedding dimensions must be 1..8192".into());
            }
            config.dimensions = dimensions;
        }
        macro_rules! optional {
            ($($field:ident),*) => { $(if let Some(value) = self.$field { config.options.$field = Some(value); })* };
        }
        optional!(
            endpoint,
            api_key_env,
            query_instruction,
            passage_instruction
        );
        if let Some(value) = self.request_timeout_secs {
            config.options.request_timeout_secs = value;
        }
        if let Some(value) = self.batch_size {
            config.options.batch_size = value;
        }
        Ok(())
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Injectable environment lookup for deterministic hosts and tests.
    pub fn from_lookup(mut get: impl FnMut(&str) -> Option<String>) -> Result<Self, String> {
        fn number<T: std::str::FromStr>(
            raw: Option<String>,
            key: &str,
        ) -> Result<Option<T>, String> {
            raw.map(|s| {
                s.parse()
                    .map_err(|_| format!("{key} must be a positive integer"))
            })
            .transpose()
        }
        Ok(Self {
            preset: get("ENGRAM_EMBEDDING_PRESET"),
            provider_type: get("ENGRAM_EMBEDDING_PROVIDER"),
            model: get("ENGRAM_EMBEDDING_MODEL"),
            dimensions: number(
                get("ENGRAM_EMBEDDING_DIMENSIONS"),
                "ENGRAM_EMBEDDING_DIMENSIONS",
            )?,
            endpoint: get("ENGRAM_EMBEDDING_ENDPOINT"),
            api_key_env: get("ENGRAM_EMBEDDING_API_KEY_ENV"),
            query_instruction: get("ENGRAM_EMBEDDING_QUERY_INSTRUCTION"),
            passage_instruction: get("ENGRAM_EMBEDDING_PASSAGE_INSTRUCTION"),
            request_timeout_secs: number(
                get("ENGRAM_EMBEDDING_TIMEOUT_SECS"),
                "ENGRAM_EMBEDDING_TIMEOUT_SECS",
            )?,
            batch_size: number(
                get("ENGRAM_EMBEDDING_BATCH_SIZE"),
                "ENGRAM_EMBEDDING_BATCH_SIZE",
            )?,
        })
    }
}
