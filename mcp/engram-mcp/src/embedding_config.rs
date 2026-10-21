//! Embedding launch options: CLI parsing delegates semantics to the SDK resolver.

use engram_integration::{EmbeddingOverrides, EmbeddingProviderConfig};

/// Resolve the runtime gate before reading any inactive provider configuration.
/// Still parse CLI syntax in off mode: typos and missing values are errors.
pub(crate) fn from_args(argv: &[String]) -> Result<(Vec<String>, EmbeddingProviderConfig), String> {
    let (remaining, _, _) = extract_args(argv)?;
    if !vectors_enabled(&remaining) {
        return Ok((remaining, EmbeddingProviderConfig::default()));
    }
    resolve_args(
        argv,
        EmbeddingOverrides::from_env()?,
        std::env::var("ENGRAM_EMBEDDING_CONFIG").ok(),
    )
}

pub(crate) fn vectors_enabled(argv: &[String]) -> bool {
    argv.iter().fold(true, |enabled, arg| match arg.as_str() {
        "--slim" | "--no-vector" => false,
        "--vector" => true,
        _ => enabled,
    })
}

fn resolve_args(
    argv: &[String],
    environment: EmbeddingOverrides,
    profile_path: Option<String>,
) -> Result<(Vec<String>, EmbeddingProviderConfig), String> {
    let (remaining, explicit, cli_path) = extract_args(argv)?;
    let path = cli_path.or(profile_path);
    let selected = path.is_some()
        || environment.provider_type.is_some()
        || environment.model.is_some()
        || environment.preset.is_some()
        || explicit.provider_type.is_some()
        || explicit.model.is_some()
        || explicit.preset.is_some();
    let profile = path
        .map(EmbeddingProviderConfig::from_toml_file)
        .transpose()?;
    let config = EmbeddingProviderConfig::resolve(profile, environment, explicit)?;
    #[cfg(not(feature = "fastembed"))]
    let config = if !selected {
        EmbeddingProviderConfig {
            provider_type: "none".to_owned(),
            model: "none".to_owned(),
            ..config
        }
    } else {
        config
    };
    #[cfg(feature = "fastembed")]
    let _ = selected;
    Ok((remaining, config))
}

fn extract_args(
    argv: &[String],
) -> Result<(Vec<String>, EmbeddingOverrides, Option<String>), String> {
    let mut remaining = Vec::new();
    let mut overrides = EmbeddingOverrides::default();
    let mut path = None;
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        if !flag.starts_with("--embedding-") {
            remaining.push(argv[i].clone());
            i += 1;
            continue;
        }
        let value = argv
            .get(i + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--embedding-config" => path = Some(value.clone()),
            "--embedding-preset" => overrides.preset = Some(value.clone()),
            "--embedding-provider" => overrides.provider_type = Some(value.clone()),
            "--embedding-model" => overrides.model = Some(value.clone()),
            "--embedding-dimensions" => {
                overrides.dimensions = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{flag} requires a positive integer"))?,
                )
            }
            "--embedding-endpoint" => overrides.endpoint = Some(value.clone()),
            "--embedding-api-key-env" => overrides.api_key_env = Some(value.clone()),
            "--embedding-query-instruction" => overrides.query_instruction = Some(value.clone()),
            "--embedding-passage-instruction" => {
                overrides.passage_instruction = Some(value.clone())
            }
            "--embedding-timeout-secs" => {
                overrides.request_timeout_secs = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{flag} requires a positive integer"))?,
                )
            }
            "--embedding-batch-size" => {
                overrides.batch_size = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{flag} requires a positive integer"))?,
                )
            }
            _ => return Err(format!("unknown argument: {flag}")),
        }
        i += 2;
    }
    Ok((remaining, overrides, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn cli_overrides_environment_and_preserves_host_arguments() {
        let environment = EmbeddingOverrides {
            model: Some("old-model".into()),
            dimensions: Some(128),
            ..Default::default()
        };
        let (remaining, config) = resolve_args(
            &args(&[
                "--storage",
                "/tmp/store",
                "--embedding-provider",
                "ollama",
                "--embedding-model",
                "new-model",
                "--embedding-endpoint",
                "http://localhost:11434",
                "--embedding-dimensions",
                "768",
                "--no-vector",
            ]),
            environment,
            None,
        )
        .unwrap();
        assert_eq!(remaining, args(&["--storage", "/tmp/store", "--no-vector"]));
        assert_eq!(config.provider_type, "ollama");
        assert_eq!(config.model, "new-model");
        assert_eq!(config.dimensions, 768);
    }

    #[test]
    fn preset_is_resolved_by_rust_sdk() {
        let (_, config) = resolve_args(
            &args(&["--embedding-preset", "local-alternative"]),
            EmbeddingOverrides::default(),
            None,
        )
        .unwrap();
        assert_eq!(config.dimensions, 768);
        assert_eq!(config.model, "BAAI/bge-base-en-v1.5");
    }

    #[test]
    fn explicit_profile_path_wins_over_environment_profile_path() {
        use std::io::Write;
        let mut profile = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            profile,
            "[embedding_provider]\npreset = \"local-alternative\""
        )
        .unwrap();
        let (_, config) = resolve_args(
            &args(&["--embedding-config", profile.path().to_str().unwrap()]),
            EmbeddingOverrides::default(),
            Some("/does/not/exist.toml".into()),
        )
        .unwrap();
        assert_eq!(config.dimensions, 768);
    }

    #[test]
    fn rejects_missing_invalid_and_unknown_arguments() {
        for values in [
            vec!["--embedding-model"],
            vec!["--embedding-dimensions", "abc"],
            vec!["--embedding-dimensions", "0"],
            vec!["--embedding-dimensions", "8193"],
            vec!["--embedding-unknown", "x"],
        ] {
            assert!(
                resolve_args(&args(&values), EmbeddingOverrides::default(), None).is_err(),
                "{values:?}"
            );
        }
    }
}
