use engram_domain::EmbeddingTargetType;
use engram_store_sqlite::{SqliteVectorIndex, VectorEntry};

fn entry(id: &str, target_id: &str, embedding: Vec<f32>) -> VectorEntry {
    VectorEntry {
        id: id.to_owned(),
        target_type: EmbeddingTargetType::Chunk,
        target_id: target_id.to_owned(),
        model: "deterministic-test".to_owned(),
        dimensions: embedding.len() as u32,
        content_hash: format!("sha256:{id}"),
        embedding,
    }
}

#[test]
fn sqlite_vec_returns_nearest_targets_in_distance_order() {
    let index = SqliteVectorIndex::open_in_memory(3).expect("open index");
    index
        .insert(entry("vec-1", "chunk-close", vec![0.0, 0.1, 0.0]))
        .expect("insert close");
    index
        .insert(entry("vec-2", "chunk-far", vec![0.9, 0.9, 0.9]))
        .expect("insert far");

    let results = index.search(&[0.0, 0.0, 0.0], 2).expect("search");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].target_id, "chunk-close");
    assert_eq!(results[1].target_id, "chunk-far");
    assert!(results[0].distance <= results[1].distance);
}

#[test]
fn sqlite_vec_rejects_dimension_mismatch() {
    let index = SqliteVectorIndex::open_in_memory(3).expect("open index");
    let error = index
        .insert(entry("vec-1", "chunk-bad", vec![0.0, 0.1]))
        .expect_err("dimension mismatch");

    assert!(error.to_string().contains("dimensions mismatch"));
}
