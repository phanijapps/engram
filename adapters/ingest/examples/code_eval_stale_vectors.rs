//! Seed synthetic retained projections only, never a model-quality assertion.
//! This deliberate stale-index fixture must stay in its isolated evaluation store.
use engram_domain::*;
use engram_knowledge::CodeSourceRepository;
use engram_store_sqlite::{SqlKnowledgeStore, SqliteVectorIndex, VectorEntry};
use futures::executor::block_on;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("isolated fixture root required")?,
    );
    let path = root.join("store/engram_data.db");
    let store = SqlKnowledgeStore::open_file(&path)?;
    let scope: Scope =
        serde_json::from_value(json!({"tenant":"default","workspace":"code-eval-denial"}))?;
    let manifests = block_on(store.list_code_sources(&scope))?;
    if manifests.len() != 2
        || manifests.iter().any(|m| {
            !["code-eval-private", "code-eval-expired"].contains(&m.file.repository.as_str())
        })
    {
        return Err("not the isolated two-source denial fixture".into());
    }
    let space = EmbeddingSpace::new(
        "fastembed",
        "BAAI/bge-small-en-v1.5",
        384,
        "query",
        None::<String>,
    );
    let index = SqliteVectorIndex::open_with_embedding_space(
        path.to_str().ok_or("fixture path is not UTF-8")?,
        space,
    )?;
    if index.requires_reindex() {
        return Err("fixture vector-space mismatch".into());
    }
    let vector = vec![1.0 / (384.0_f32).sqrt(); 384];
    let mut ids = Vec::new();
    for manifest in manifests {
        for record in manifest
            .records
            .iter()
            .filter(|r| r.kind == CodeSourceRecordKind::Chunk)
        {
            // Normal canonical views already hide expired chunks. The verified
            // source inventory remains authoritative for this deliberate stale
            // projection; never bypass that view in runtime retrieval.
            if manifest
                .snapshot_ids
                .iter()
                .any(|id| id.as_str() == record.id)
            {
                continue;
            }
            index.insert(VectorEntry {
                id: record.id.clone(),
                target_type: EmbeddingTargetType::Chunk,
                target_id: record.id.clone(),
                model: "BAAI/bge-small-en-v1.5".into(),
                dimensions: 384,
                content_hash: "synthetic-stale-projection".into(),
                embedding: vector.clone(),
            })?;
            ids.push(record.id.clone());
        }
    }
    let hits = index.search(&vector, 10)?;
    if ids.len() != 4
        || ids
            .iter()
            .any(|id| !hits.iter().any(|h| h.target_id == *id))
    {
        return Err("retained vector positive control failed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"synthetic":true,"purpose":"stale ownership filtering; not embedding/model quality", "retainedTargets":ids,"rawNearestHitCount":hits.len()})
        )?
    );
    Ok(())
}
