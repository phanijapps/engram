#![cfg(feature = "sqlite")]
//! Actual scanner/knowledge/vector integration with deterministic local inference.
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_integration::{CapabilityReport, EmbeddingProvider, EngramProviderBuilder};
use engram_knowledge::{
    CodeSegmentRepository, CodeSourceRepository, KnowledgeRepository, code_segment_text,
};
use engram_retrieval::{CodeIntelligenceConfig, TextTokenization, VectorIndex};
use engram_runtime::CoreResult;
use engram_store_sqlite::{SqlKnowledgeStore, SqliteVectorIndex};
use futures::executor::block_on;
use serde_json::json;
use std::sync::{Arc, Mutex};

struct ScalarModel {
    inputs: Mutex<Vec<String>>,
}
impl EmbeddingProvider for ScalarModel {
    fn is_local(&self) -> bool {
        true
    }
    fn provider_id(&self) -> &str {
        "local-test"
    }
    fn model_id(&self) -> &str {
        "scalar-fixture"
    }
    fn dimensions(&self) -> u32 {
        3
    }
    fn embedding_space(&self) -> EmbeddingSpace {
        EmbeddingSpace::new("local-test", "scalar-fixture", 3, "passage", None::<String>)
    }
    fn tokenize_untruncated(&self, text: &str, specials: bool) -> CoreResult<TextTokenization> {
        Ok(TextTokenization {
            offsets: text
                .char_indices()
                .map(|(start, ch)| CodeByteRange {
                    start: start as u64,
                    end: (start + ch.len_utf8()) as u64,
                })
                .collect(),
            special_tokens: if specials { 2 } else { 0 },
            window: 512,
            tokenizer_id: "scalar-fixture-v1".into(),
        })
    }
    fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
        Ok(vec![1., 0., 0.])
    }
    fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        self.inputs.lock().unwrap().push(text.into());
        Ok(vec![1., 0., 0.])
    }
}

#[test]
fn bounded_reindex_covers_large_unicode_method_and_ignores_canonical_vector_donors() {
    let root = tempfile::tempdir().unwrap();
    let text = format!(
        "pub fn enormous() {{\n let mut total = 0;\n{}\n return FINAL_TAIL_ANCHOR;\n}}\n",
        " total += 1; // Unicode π 🦀\n".repeat(420)
    );
    assert!(text.len() > 8192);
    std::fs::write(root.path().join("a.rs"), &text).unwrap();
    for name in ["b.rs", "c.rs"] {
        std::fs::write(root.path().join(name), "pub fn unavailable() {}\n").unwrap();
    }
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let options = ScanOptions {
        scope: serde_json::from_value(json!({"tenant":"t","workspace":"w"})).unwrap(),
        policy: serde_json::from_value(
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
        )
        .unwrap(),
        actor: serde_json::from_value(json!({"id":"owner","kind":"agent"})).unwrap(),
        source_name: "fixture".into(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: Default::default(),
    };
    let (scanned, _) = scan_repository(root.path(), &options, store.as_ref(), |_| {}).unwrap();
    assert_eq!(scanned.errors, 0, "{scanned:?}");
    let sources = block_on(store.list_code_sources(&options.scope)).unwrap();
    let source = sources
        .iter()
        .find(|m| m.file.path == "a.rs")
        .unwrap()
        .clone();
    for old in sources.iter().filter(|m| m.file.path != "a.rs") {
        let mut updating = old.clone();
        updating.state = CodeSourceState::Updating;
        updating.attempt_id = format!("unavailable-{}", old.file.path);
        block_on(store.begin_code_source(updating.clone(), Some(&old.attempt_id))).unwrap();
        if old.file.path == "c.rs" {
            block_on(store.fail_code_source(&updating.file, &updating.attempt_id)).unwrap();
        }
    }
    let parents: Vec<_> = block_on(store.list_chunks(&options.scope))
        .unwrap()
        .into_iter()
        .filter(|p| p.code.as_ref().is_some_and(|c| c.file == source.file))
        .collect();
    let model = Arc::new(ScalarModel {
        inputs: Mutex::new(Vec::new()),
    });
    let representation = engram_integration::code_representation::code_representation(
        model.as_ref(),
        &CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1).index,
    )
    .unwrap();
    let base = Arc::new(
        SqliteVectorIndex::open_in_memory(3)
            .unwrap()
            .with_embedding_space(model.embedding_space()),
    );
    for id in source
        .snapshot_ids
        .iter()
        .chain(parents.iter().map(|p| &p.id))
    {
        block_on(VectorIndex::insert(
            base.as_ref(),
            id,
            &model.embedding_space(),
            vec![1., 0., 0.],
        ))
        .unwrap();
    }
    let code = Arc::new(
        SqliteVectorIndex::open_in_memory(3)
            .unwrap()
            .with_embedding_space(representation.embedding_space.clone()),
    );
    let provider = EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .knowledge(store.clone())
        .knowledge_query(store.clone())
        .embedding_provider(model.clone())
        .vectors(base.clone())
        .code_vectors(code.clone())
        .build();
    let request = EmbeddingBackfillRequest {
        scope: options.scope.clone(),
        requester: serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap(),
        limit: 7,
        source_filter: None,
    };
    assert_eq!(
        provider.capabilities().code_embeddings,
        CapabilityState::Supported
    );
    let missing_generic =
        EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
            .knowledge(store.clone())
            .knowledge_query(store.clone())
            .embedding_provider(model.clone())
            .code_vectors(code.clone())
            .build();
    assert!(matches!(
        missing_generic.capabilities().code_embeddings,
        CapabilityState::Unsupported { .. }
    ));
    // Simulate a process dying after the durable begin but before segment writes.
    let interrupted = CodeSegmentInventory {
        file: source.file.clone(),
        generation: source.generation.clone(),
        attempt_id: source.attempt_id.clone(),
        representation_id: representation.id.clone(),
        segment_ids: vec!["never-written".into()],
        state: CodeSourceState::Updating,
    };
    block_on(store.begin_code_segments(interrupted, None)).unwrap();
    let first = block_on(provider.backfill_embeddings(&request)).unwrap();
    assert_eq!(first.embedded, 7);
    assert!(first.remaining > 0);
    let mut inventory = block_on(store.code_segment_inventory(&source.file, &representation.id))
        .unwrap()
        .unwrap();
    assert_eq!(inventory.inventory.state, CodeSourceState::Updating);
    assert!(
        block_on(store.resolve_code_segment(
            &inventory.inventory.segment_ids[0],
            &request.scope,
            &request.requester,
            &representation.id
        ))
        .unwrap()
        .is_none(),
        "partial representation is not query-ready"
    );
    // A failed partial attempt also recovers without discarding completed vectors.
    block_on(store.fail_code_segments(&inventory)).unwrap();
    let mut total = first.embedded;
    for _ in 0..100 {
        let report = block_on(provider.backfill_embeddings(&request)).unwrap();
        total += report.embedded;
        if report.remaining == 0 {
            break;
        }
    }
    inventory = block_on(store.code_segment_inventory(&source.file, &representation.id))
        .unwrap()
        .unwrap();
    assert_eq!(inventory.inventory.state, CodeSourceState::Active);
    assert_eq!(total, inventory.inventory.segment_ids.len());
    assert_eq!(
        model.inputs.lock().unwrap().len(),
        total,
        "canonical/snapshot vectors cannot substitute for new representation inputs"
    );
    assert!(
        model
            .inputs
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.chars().count() <= 384)
    );
    let set = block_on(store.load_current_code_segments(
        &source.file,
        &representation.id,
        &request.requester,
    ))
    .unwrap()
    .unwrap();
    for parent in &parents {
        let bodies: Vec<String> = set
            .segments
            .iter()
            .filter(|s| s.parent_chunk_id == parent.id)
            .map(|s| code_segment_text(parent, &s.ranges).unwrap())
            .collect();
        assert_eq!(
            bodies.concat(),
            parent.text,
            "every canonical byte, including the method tail, stays covered"
        );
        assert_eq!(
            block_on(store.get_chunk(&parent.id, &request.scope))
                .unwrap()
                .unwrap(),
            *parent
        );
    }
    assert!(
        model
            .inputs
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.contains("FINAL_TAIL_ANCHOR"))
    );
    assert!(
        block_on(code.embedded_ids())
            .unwrap()
            .iter()
            .all(|id| id.as_str().starts_with("segment-"))
    );
    let repeated = block_on(provider.backfill_embeddings(&request)).unwrap();
    assert_eq!(repeated.embedded, 0);
    assert_eq!(repeated.new_embeddings, 0);
    assert_eq!(repeated.remaining, 0);
}
