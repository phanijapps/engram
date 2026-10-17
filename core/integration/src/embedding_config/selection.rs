use super::{EmbeddingOverrides, EmbeddingProviderConfig};
use serde::{Deserialize, Deserializer};
use std::path::Path;

impl EmbeddingProviderConfig {
    pub fn preset(name: &str) -> Result<Self, String> {
        let mut config = Self::default();
        match name {
            "default" => {}
            "local-alternative" => {
                config.model = "BAAI/bge-base-en-v1.5".into();
                config.dimensions = 768;
            }
            "multilingual" => {
                config.model = "BAAI/bge-m3".into();
                config.dimensions = 1024;
            }
            "code-evaluation" => {
                config.provider_type = "openai-compatible".into();
                config.model = "Qwen/Qwen3-Embedding-0.6B".into();
                config.dimensions = 1024;
                config.options.query_instruction = Some("Instruct: Given a question about a codebase, retrieve relevant code that answers the question\nQuery: ".into());
            }
            _ => return Err(format!("unknown embedding preset: {name}")),
        }
        Ok(config)
    }

    /// Shared precedence: explicit > environment > supplied profile > defaults.
    pub fn resolve(
        profile: Option<Self>,
        environment: EmbeddingOverrides,
        explicit: EmbeddingOverrides,
    ) -> Result<Self, String> {
        let mut config = profile.unwrap_or_default();
        environment.apply(&mut config)?;
        explicit.apply(&mut config)?;
        config.validate()?;
        Ok(config)
    }

    /// Reads only the embedding section; other host-profile sections stay host-owned.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Profile {
            #[serde(default)]
            embedding_provider: EmbeddingProviderConfig,
        }
        let text = std::fs::read_to_string(path)
            .map_err(|_| "cannot read embedding profile".to_string())?;
        let profile: Profile =
            toml::from_str(&text).map_err(|_| "invalid embedding profile TOML".to_string())?;
        profile.embedding_provider.validate()?;
        Ok(profile.embedding_provider)
    }
}

impl<'de> Deserialize<'de> for EmbeddingProviderConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Input {
            #[serde(flatten)]
            overrides: EmbeddingOverrides,
            prompt_profile: Option<String>,
            normalization: Option<String>,
        }
        let input = Input::deserialize(deserializer)?;
        let mut config = Self::default();
        input
            .overrides
            .apply(&mut config)
            .map_err(serde::de::Error::custom)?;
        if let Some(value) = input.prompt_profile {
            config.prompt_profile = value;
        }
        config.normalization = input.normalization;
        config.validate().map_err(serde::de::Error::custom)?;
        Ok(config)
    }
}
