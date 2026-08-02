#![cfg(feature = "fastembed-tests")]

use engram_domain::*;
use engram_store_sqlite::{
    FastEmbedBgeSmallQueryProvider, SqliteVectorIndex, VectorEntry, VectorQueryProvider,
};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

#[test]
#[ignore = "downloads FastEmbed BGE-small model assets"]
fn fastembed_bge_small_vectors_query_sqlite_vec() {
    let mut model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
    )
    .expect("initialize BGE-small");
    let passages = vec![
        "passage: Engram stores source-grounded knowledge chunks.",
        "passage: The weather forecast is unrelated to memory architecture.",
    ];
    let passage_embeddings = model.embed(passages, None).expect("embed passages");
    let query_provider = FastEmbedBgeSmallQueryProvider::new().expect("initialize query provider");
    let query_embedding = query_provider
        .query_vector(&request("How does Engram store knowledge chunks?"))
        .expect("embed query");

    let dimensions = query_embedding.len() as u32;
    let index = SqliteVectorIndex::open_in_memory(dimensions).expect("open index");
    for (idx, embedding) in passage_embeddings.into_iter().enumerate() {
        index
            .insert(VectorEntry {
                id: format!("bge-small-{idx}"),
                target_type: EmbeddingTargetType::Chunk,
                target_id: format!("chunk-{idx}"),
                model: "fastembed/bge-small-en-v1.5".to_owned(),
                dimensions,
                content_hash: format!("sha256:bge-small-{idx}"),
                embedding,
            })
            .expect("insert vector");
    }

    let results = index.search(&query_embedding, 1).expect("search");

    assert_eq!(results[0].target_id, "chunk-0");
}

/// T2 (indexing-embed-performance): `embed_passage_batch` is the native FastEmbed
/// batch path. It must (a) return one vector per input, (b) match
/// `embed_passage` per-item within tolerance, and (c) handle empty input.
#[test]
#[ignore = "downloads FastEmbed BGE-small model assets"]
fn fastembed_embed_passage_batch_matches_per_item_within_tolerance() {
    let provider = FastEmbedBgeSmallQueryProvider::new().expect("initialize provider");

    // Empty input ⇒ empty output (no model call).
    assert!(provider.embed_passage_batch(&[]).unwrap().is_empty());

    let texts: Vec<String> = [
        "Engram stores source-grounded knowledge chunks.",
        "The weather forecast is unrelated to memory architecture.",
        "Rust owns deterministic behavior in the core layer.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // Per-item embeddings (the reference).
    let per_item: Vec<Vec<f32>> = texts
        .iter()
        .map(|t| provider.embed_passage(t).expect("embed_passage"))
        .collect();

    // Batched embeddings.
    let batched = provider
        .embed_passage_batch(&texts)
        .expect("embed_passage_batch");

    assert_eq!(
        batched.len(),
        texts.len(),
        "batch must return one embedding per input"
    );
    for (i, (a, b)) in per_item.iter().zip(batched.iter()).enumerate() {
        assert_eq!(a.len(), b.len(), "dimension mismatch at {i}");
        let max_delta = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        // Same model, same text → embeddings must agree to within f32 batch noise.
        assert!(
            max_delta < 1e-4,
            "batched embedding {i} diverges from per-item by {max_delta}"
        );
    }
}

/// T2: empty/whitespace texts are skipped inside the batch and their slots come
/// back empty (the ingest loop already skips empty text; this keeps the batch
/// path aligned so it never feeds an empty string to the model).
#[test]
#[ignore = "downloads FastEmbed BGE-small model assets"]
fn fastembed_embed_passage_batch_skips_empty_slots() {
    let provider = FastEmbedBgeSmallQueryProvider::new().expect("initialize provider");
    let texts: Vec<String> = vec![
        "Engram stores source-grounded knowledge chunks.".to_string(),
        "   ".to_string(),
        "Rust owns deterministic behavior.".to_string(),
    ];
    let batched = provider
        .embed_passage_batch(&texts)
        .expect("embed_passage_batch");
    assert_eq!(batched.len(), 3);
    assert!(!batched[0].is_empty(), "non-empty text must embed");
    assert!(batched[1].is_empty(), "whitespace-only slot must be empty");
    assert!(!batched[2].is_empty(), "non-empty text must embed");
}

fn request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: Scope {
            tenant: "tenant-demo".to_owned(),
            subject: None,
            workspace: Some("engram".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        },
        requester: Requester {
            actor: Actor {
                id: Id::from("actor-fastembed"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes: vec![RetrievalMode::Semantic],
        filters: None,
        cues: Vec::new(),
        limit: Some(1),
        budget: None,
        include_explanations: Some(true),
    }
}
