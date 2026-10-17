//! JSON transport for the shared embedding configuration resolver.

use engram_integration::{EmbeddingOverrides, EmbeddingProviderConfig};
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Resolve without loading a model or reading API credentials. Ambient
/// environment overrides are opt-in so explicit SDK callers remain deterministic.
#[napi]
pub fn resolve_embedding_config(
    profile_json: Option<String>,
    overrides_json: Option<String>,
    use_environment: Option<bool>,
) -> Result<String> {
    let profile = profile_json.as_deref().map(crate::decode).transpose()?;
    let explicit = overrides_json
        .as_deref()
        .map(crate::decode)
        .transpose()?
        .unwrap_or_default();
    let environment = if use_environment.unwrap_or(false) {
        EmbeddingOverrides::from_env().map_err(Error::from_reason)?
    } else {
        EmbeddingOverrides::default()
    };
    let resolved = EmbeddingProviderConfig::resolve(profile, environment, explicit)
        .map_err(Error::from_reason)?;
    crate::encode(&resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_and_presets_without_model_initialization() {
        let value: serde_json::Value =
            serde_json::from_str(&resolve_embedding_config(None, None, Some(false)).unwrap())
                .unwrap();
        assert_eq!(value["model"], "BAAI/bge-small-en-v1.5");
        assert_eq!(value["dimensions"], 384);
        let value: serde_json::Value = serde_json::from_str(
            &resolve_embedding_config(
                None,
                Some(r#"{"preset":"local-alternative"}"#.into()),
                Some(false),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["model"], "BAAI/bge-base-en-v1.5");
        assert_eq!(value["dimensions"], 768);
    }

    #[test]
    fn explicit_overrides_win_over_partial_profile() {
        let value: serde_json::Value = serde_json::from_str(
            &resolve_embedding_config(
                Some(r#"{"preset":"local-alternative"}"#.into()),
                Some(r#"{"model":"custom","provider_type":"ollama","endpoint":"http://localhost:11434","dimensions":512}"#.into()),
                Some(false),
            ).unwrap(),
        ).unwrap();
        assert_eq!(value["model"], "custom");
        assert_eq!(value["dimensions"], 512);
    }

    #[test]
    fn rejects_malformed_or_invalid_inputs() {
        for json in [
            "invalid-json",
            r#"{"dimensions":0}"#,
            r#"{"dimensions":8193}"#,
        ] {
            assert!(resolve_embedding_config(None, Some(json.into()), Some(false)).is_err());
        }
    }
}
