use engram_domain::{CodeIntelligenceProfile, RerankStrategy};
use engram_integration::code_configuration::{
    CodeConfigLaunch, load_code_config, resolve_code_config,
};
use engram_retrieval::CodeIntelligenceConfigOverlay;
use std::collections::BTreeMap;

fn overlay(value: serde_json::Value) -> CodeIntelligenceConfigOverlay {
    serde_json::from_value(value).unwrap()
}

#[test]
fn profile_selection_precedes_fieldwise_precedence_and_preserves_falsy_fields() {
    let file = overlay(
        serde_json::json!({"profile":"balanced-v1", "retrieval":{"graph_hops":0}, "fusion":{"source_weights":{"vector":0.0}}}),
    );
    let env = overlay(
        serde_json::json!({"profile":"fast-v1", "retrieval":{"candidates_per_lane":35}, "fusion":{"rrf_k":42}}),
    );
    let explicit = overlay(
        serde_json::json!({"evidence":{"token_budget":1024}, "fusion":{"source_weights":{"lexical":2.0}}}),
    );
    let r = resolve_code_config(Some(&file), Some(&env), Some(&explicit)).unwrap();
    assert_eq!(r.effective.profile, CodeIntelligenceProfile::FastV1);
    assert_eq!(
        r.effective.fusion.rerank.unwrap().strategy,
        RerankStrategy::None
    );
    assert_eq!(r.effective.retrieval.candidates_per_lane, 35);
    assert_eq!(r.effective.retrieval.graph_hops, 0);
    assert_eq!(r.effective.fusion.default_source_weight, 1.0);
    assert_eq!(r.effective.fusion.source_weights["vector"], 0.0);
    assert_eq!(r.effective.fusion.source_weights["lexical"], 2.0);
    assert_eq!(r.origins["fusion.rrf_k"], "environment");
    assert_eq!(r.origins["retrieval.graph_hops"], "file");
    assert_eq!(r.origins["evidence.token_budget"], "explicit");
}

#[test]
fn json_toml_and_environment_produce_the_same_effective_hash() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("config.json");
    let toml = dir.path().join("config.toml");
    std::fs::write(&json, r#"{"profile":"fast-v1","fusion":{"rrf_k":42}}"#).unwrap();
    std::fs::write(&toml, "profile = \"fast-v1\"\n[fusion]\nrrf_k = 42\n").unwrap();
    let a = load_code_config(&CodeConfigLaunch {
        file: Some(json),
        ..Default::default()
    })
    .unwrap();
    let b = load_code_config(&CodeConfigLaunch {
        file: Some(toml),
        ..Default::default()
    })
    .unwrap();
    let c = load_code_config(&CodeConfigLaunch {
        environment: BTreeMap::from([
            ("ENGRAM_CODE_PROFILE".into(), "fast-v1".into()),
            ("ENGRAM_CODE__FUSION__RRF_K".into(), "42".into()),
        ]),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.hash, c.hash);
    assert_ne!(a.origins, c.origins);
}

#[test]
fn unknown_or_null_settings_and_bad_environment_are_rejected() {
    for value in [
        serde_json::json!({"query":null}),
        serde_json::json!({"fusion":{"rerank":null}}),
        serde_json::json!({"fusion":{"rerank":{"x":0}}}),
    ] {
        assert!(serde_json::from_value::<CodeIntelligenceConfigOverlay>(value).is_err());
    }
    for (key, value) in [
        ("ENGRAM_CODE__RETRIEVAL__CANDIDATES_PER_LANE", "NaN"),
        ("ENGRAM_CODE__INDEX__TYPO", "5"),
        ("ENGRAM_CODE__FUSION__RERANK__STRATEGY", "mmr"),
        ("ENGRAM_CODE_WHATEVER", "x"),
        ("ENGRAM_CODE_PROFILE", "future-v9"),
    ] {
        let launch = CodeConfigLaunch {
            environment: BTreeMap::from([(key.into(), value.into())]),
            ..Default::default()
        };
        assert!(load_code_config(&launch).is_err(), "{key}");
    }
}

#[test]
fn bad_limits_identify_the_field_and_winning_layer() {
    let err = load_code_config(&CodeConfigLaunch {
        environment: BTreeMap::from([(
            "ENGRAM_CODE__RETRIEVAL__CANDIDATES_PER_LANE".into(),
            "5".into(),
        )]),
        ..Default::default()
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("retrieval.candidates_per_lane from environment"),
        "{err}"
    );
    let err = resolve_code_config(
        Some(&overlay(
            serde_json::json!({"retrieval":{"result_limit":51}}),
        )),
        None,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("retrieval.result_limit from file"), "{err}");
    let err = load_code_config(&CodeConfigLaunch {
        environment: BTreeMap::from([(
            "ENGRAM_CODE__FUSION__SOURCE_WEIGHTS__VECTORS".into(),
            "1".into(),
        )]),
        ..Default::default()
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("fusion.source_weights.vectors from environment"),
        "{err}"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, r#"{"local_scorer":{"batch_size":"eight"}}"#).unwrap();
    let err = load_code_config(&CodeConfigLaunch {
        file: Some(path),
        ..Default::default()
    })
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("local_scorer.batch_size") && err.contains("from file"),
        "{err}"
    );
}

#[test]
fn host_profile_code_section_preserves_file_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.toml");
    std::fs::write(&path,format!("[backend]\nkind=\"sqlite\"\ndata_root={:?}\n[code_intelligence]\nprofile=\"fast-v1\"\n[code_intelligence.fusion]\nrrf_k=42\n", dir.path())).unwrap();
    let config = engram_integration::EngramConfig::from_profile_file(&path).unwrap();
    let resolved = load_code_config(&CodeConfigLaunch {
        file_overlay: config.code_profile_settings.clone(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(resolved.effective.fusion.rrf_k, 42);
    assert_eq!(resolved.origins["fusion.rrf_k"], "file");
    // A code-only path cannot silently ignore unknown host/root sections.
    assert!(
        load_code_config(&CodeConfigLaunch {
            file: Some(path),
            ..Default::default()
        })
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn environment_capture_ignores_non_unicode_outside_code_namespace() {
    use std::os::unix::ffi::OsStringExt;
    if let Ok(expect_error) = std::env::var("ENGRAM_TEST_ENV_CHILD") {
        let dir = tempfile::tempdir().unwrap();
        let c = engram_integration::EngramConfig::new(
            dir.path(),
            dir.path(),
            engram_domain::ScopeMappingStrategy::Strict,
            engram_integration::EmbeddingProviderConfig {
                provider_type: "none".into(),
                model: "none".into(),
                dimensions: 384,
                prompt_profile: "query".into(),
                normalization: None,
            },
            engram_integration::MigrationMode::DryRun,
            engram_integration::CapabilityPolicy::FailClosed,
        );
        assert_eq!(
            c.resolve_code_configuration().is_err(),
            expect_error == "true"
        );
        return;
    }
    for (key, error) in [("UNRELATED_VALUE", false), ("ENGRAM_CODE_PROFILE", true)] {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "environment_capture_ignores_non_unicode_outside_code_namespace",
        ]);
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("ENGRAM_CODE") {
                cmd.env_remove(k);
            }
        }
        let status = cmd
            .env("ENGRAM_TEST_ENV_CHILD", error.to_string())
            .env(key, std::ffi::OsString::from_vec(vec![0xff]))
            .status()
            .unwrap();
        assert!(status.success());
    }
}
