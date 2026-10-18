use engram_integration::EmbeddingProviderConfig;

#[test]
fn partial_embedding_json_selects_the_requested_model() {
    let config: EmbeddingProviderConfig =
        serde_json::from_str(r#"{"model":"BAAI/bge-base-en-v1.5"}"#)
            .expect("partial model configuration is supported");
    assert_eq!(config.provider_type, "fastembed");
    assert_eq!(config.model, "BAAI/bge-base-en-v1.5");
    assert_eq!(
        config.dimensions, 0,
        "omitted dimensions resolve from the selected model"
    );
}

#[test]
fn defaults_legacy_json_and_auto_roundtrip_preserve_settings() {
    let default = EmbeddingProviderConfig::default();
    let legacy: EmbeddingProviderConfig = serde_json::from_str(r#"{"provider_type":"fastembed","model":"BAAI/bge-small-en-v1.5","dimensions":384,"prompt_profile":"query","normalization":null}"#).unwrap();
    assert_eq!(legacy, default);
    let auto: EmbeddingProviderConfig =
        serde_json::from_str(r#"{"provider_type":"ollama","model":"custom-model"}"#).unwrap();
    assert_eq!(auto.dimensions, 0);
    assert_eq!(
        serde_json::from_value::<EmbeddingProviderConfig>(serde_json::to_value(&auto).unwrap())
            .unwrap(),
        auto
    );
}

#[test]
fn all_presets_and_explicit_override_precedence() {
    use engram_integration::EmbeddingOverrides;
    for (name, dimensions) in [
        ("default", 384),
        ("local-alternative", 768),
        ("code-evaluation", 1024),
        ("multilingual", 1024),
    ] {
        assert_eq!(
            EmbeddingProviderConfig::preset(name).unwrap().dimensions,
            dimensions
        );
        let parsed: EmbeddingProviderConfig =
            serde_json::from_value(serde_json::json!({"preset":name})).unwrap();
        assert_eq!(parsed, EmbeddingProviderConfig::preset(name).unwrap());
    }
    let env = EmbeddingOverrides::from_lookup(|key| match key {
        "ENGRAM_EMBEDDING_PRESET" => Some("multilingual".into()),
        "ENGRAM_EMBEDDING_BATCH_SIZE" => Some("8".into()),
        _ => None,
    })
    .unwrap();
    let explicit: EmbeddingOverrides = serde_json::from_value(serde_json::json!({"model":"custom","provider_type":"ollama","dimensions":256,"endpoint":"http://127.0.0.1:11434"})).unwrap();
    let config = EmbeddingProviderConfig::resolve(
        Some(EmbeddingProviderConfig::preset("local-alternative").unwrap()),
        env,
        explicit,
    )
    .unwrap();
    assert_eq!(config.model, "custom");
    assert_eq!(config.dimensions, 256);
    assert_eq!(config.options.batch_size, 8);
}

#[test]
fn invalid_configuration_is_not_silently_defaulted() {
    for raw in [
        r#"{"dimensions":0}"#,
        r#"{"dimensions":-1}"#,
        r#"{"dimensions":8193}"#,
        r#"{"preset":"missing"}"#,
        r#"{"model":""}"#,
        r#"{"batch_size":0}"#,
        r#"{"request_timeout_secs":301}"#,
    ] {
        assert!(
            serde_json::from_str::<EmbeddingProviderConfig>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn semantic_options_change_identity_but_credentials_and_tuning_do_not() {
    let base = EmbeddingProviderConfig::default();
    assert_eq!(
        base.embedding_space(),
        engram_domain::EmbeddingSpace::new(
            "fastembed",
            "BAAI/bge-small-en-v1.5",
            384,
            "query",
            None::<String>
        )
    );
    let mut config = base.clone();
    config.options.batch_size = 5;
    config.options.request_timeout_secs = 60;
    config.options.api_key_env = Some("EXAMPLE_KEY".into());
    assert_eq!(config.embedding_space(), base.embedding_space());
    config.options.query_instruction = Some("different query: ".into());
    assert_ne!(config.embedding_space(), base.embedding_space());
    let mut passage = base.clone();
    passage.options.passage_instruction = Some("different passage: ".into());
    assert_ne!(passage.embedding_space(), base.embedding_space());
    let mut endpoint = base.clone();
    endpoint.options.endpoint = Some("https://example.invalid".into());
    assert_ne!(endpoint.embedding_space(), base.embedding_space());
}

#[test]
fn disabled_vectors_do_not_contact_a_served_endpoint_or_resolve_credentials() {
    let mut config: EmbeddingProviderConfig = serde_json::from_str(r#"{"provider_type":"openai-compatible","model":"unavailable","endpoint":"http://127.0.0.1:1","api_key_env":"NO_SUCH_EMBEDDING_TEST_KEY"}"#).unwrap();
    assert!(
        engram_integration::prepare_embedding(&config, false)
            .unwrap()
            .provider
            .is_none()
    );
    config.provider_type = "unknown".into();
    assert!(engram_integration::prepare_embedding(&config, true).is_err());
}

#[cfg(feature = "fastembed")]
#[test]
fn local_dimension_mismatch_fails_before_model_initialization() {
    let config = EmbeddingProviderConfig {
        dimensions: 768,
        ..Default::default()
    };
    assert!(engram_integration::prepare_embedding(&config, true).is_err());
}

#[test]
fn configured_local_models_do_not_reuse_legacy_mislabeled_spaces() {
    let config = EmbeddingProviderConfig::preset("local-alternative").unwrap();
    let legacy = engram_domain::EmbeddingSpace::new(
        "fastembed",
        "BAAI/bge-base-en-v1.5",
        768,
        "query",
        None::<String>,
    );
    assert_ne!(config.embedding_space(), legacy);
}
