use engram_domain::KnowledgeChunk;
use serde_json::{Value, json};

fn chunk() -> Value {
    json!({
        "id":"chunk-test", "documentId":"doc-test", "sourceId":"source-test",
        "kind":"code_symbol", "text":"fn run() {}", "contentHash":"sha256:test",
        "provenance":{"source":"test","actor":{"id":"actor-test","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"},
        "policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]},
        "createdAt":"2026-10-14T00:00:00Z"
    })
}

fn descriptor() -> Value {
    json!({
        "role":"symbol", "file":{"scope":{"tenant":"tenant","workspace":"project"},"repository":"repo","path":"src/main.rs"},
        "generation":"generation-1", "sourceHash":"sha256:file", "fingerprint":"code-source-v1",
        "ranges":[{"start":0,"end":11}], "complete":true,
        "parseQuality":"clean", "unsupportedFeatures":[]
    })
}

#[test]
fn typed_code_descriptor_round_trips_without_metadata() {
    let mut input = chunk();
    input["code"] = descriptor();
    input["documentKind"] = json!("code");
    let parsed: KnowledgeChunk = serde_json::from_value(input.clone()).unwrap();
    let output = serde_json::to_value(parsed).unwrap();
    assert_eq!(output["code"], input["code"]);
    assert_eq!(output["documentKind"], json!("code"));
}

#[test]
fn legacy_chunk_has_no_invented_code_coverage() {
    let parsed: KnowledgeChunk = serde_json::from_value(chunk()).unwrap();
    let output = serde_json::to_value(parsed).unwrap();
    assert!(output.get("code").is_none());
}

#[test]
fn invalid_code_roles_ranges_and_unknown_fields_are_rejected() {
    for mutation in [0, 1, 2, 3, 4] {
        let mut input = chunk();
        let mut code = descriptor();
        match mutation {
            0 => code["role"] = json!("unknown_role"),
            1 => code["ranges"] = json!([{"start":20,"end":2}]),
            2 => code["untrustedExtraField"] = json!(true),
            3 => code["generation"] = json!(""),
            _ => code["ranges"] = json!([]),
        }
        input["code"] = code;
        assert!(serde_json::from_value::<KnowledgeChunk>(input).is_err());
    }
}
