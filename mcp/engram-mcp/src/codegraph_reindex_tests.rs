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

/// Missing lanes are not an empty backlog. Never claim success without
/// actually being able to inspect and update the index.
#[test]
fn reindex_without_vector_lane_is_an_error() {
    let app = test_app(true);
    assert!(reindex(&app, &json!({})).is_err());
    assert!(reindex(&app, &json!({"limit": 5})).is_err());
}
