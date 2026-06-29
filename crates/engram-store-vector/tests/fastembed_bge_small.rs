#![cfg(feature = "fastembed-tests")]

use engram_domain::EmbeddingTargetType;
use engram_store_vector::{SqliteVectorIndex, VectorEntry};
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
    let query_embedding = model
        .embed(vec!["query: How does Engram store knowledge chunks?"], None)
        .expect("embed query")
        .remove(0);

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
