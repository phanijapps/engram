//! Model-independent MCP reindex handler contract tests.
use super::*;
use engram_integration::{CapabilityReport, EmbeddingProvider, EngramProviderBuilder};
use engram_knowledge::CoreResult;
use serde_json::json;
use std::sync::Arc;

// These tests cover the MCP handler, independently of installed model assets.
// Real vector backfill is verified by the disposable-store live smoke.
struct UnusedEmbedder;
impl EmbeddingProvider for UnusedEmbedder {
    fn provider_id(&self) -> &str {
        "test"
    }
    fn model_id(&self) -> &str {
        "unused"
    }
    fn dimensions(&self) -> u32 {
        1
    }
    fn embedding_space(&self) -> engram_domain::EmbeddingSpace {
        engram_domain::EmbeddingSpace::new("test", "unused", 1, "query", None::<String>)
    }
    fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
        panic!("empty backfill must not call inference")
    }
    fn embed_passage(&self, _: &str) -> CoreResult<Vec<f32>> {
        panic!("empty backfill must not call inference")
    }
}

fn test_app(with_embedder: bool) -> App {
    let builder = EngramProviderBuilder::new(CapabilityReport::builder().build());
    let provider = if with_embedder {
        builder.embedding_provider(Arc::new(UnusedEmbedder)).build()
    } else {
        builder.build()
    };
    App {
        provider,
        scope: crate::scope::project_scope("test-project", "default"),
        ontology: Default::default(),
        taxonomy: Default::default(),
        storage_dir: Default::default(),
    }
}

#[test]
fn reindex_without_provider_reports_unavailable_without_fake_progress() {
    let app = test_app(false);
    let result = reindex(&app, &json!({"limit": 5})).unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("no embedding provider wired"), "{text}");
    assert!(!text.contains("backlog drained"), "{text}");
    assert!(!text.contains("embedded 0 chunks"), "{text}");
}

#[test]
fn reindex_rejects_wire_scope_widening_before_processing() {
    let app = test_app(false);
    let error = reindex(&app, &json!({"scope":{"tenant":"other"}})).unwrap_err();
    assert!(error.message.contains("trusted launch scope"));
}

/// PS5: the handler renders an empty progress report when an embedder is
/// available but no backfill lanes are attached. Its scope defaults to the
/// launch scope when omitted (the maintenance tools' AC2 contract).
#[test]
fn reindex_answers_with_progress_shape_and_default_scope() {
    let app = test_app(true);
    let res = reindex(&app, &json!({})).unwrap();
    let text = res["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with("reindex:") && text.contains("embedded 0 chunks"),
        "progress-report shape: {text}"
    );
    // With an embedder and no backfill lanes, the empty report still
    // preserves the requested limit. The unavailable path has its own test.
    let res2 = reindex(&app, &json!({ "limit": 5 })).unwrap();
    assert!(
        res2["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("limit 5"),
        "limit threads through: {}",
        res2["content"][0]["text"].as_str().unwrap()
    );
}
