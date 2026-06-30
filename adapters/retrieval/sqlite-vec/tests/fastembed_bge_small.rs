#![cfg(feature = "fastembed-tests")]

use engram_domain::*;
use engram_store_vector::{
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
