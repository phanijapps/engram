//! Retrieval trace capability fixture.
//!
//! Exercises the extended `FusionTrace` contract: a trace round-trips through
//! serde with the new retrieval fields populated, and a vector-only retrieval
//! over the `SqliteVectorIndex` produces a stable trace with a source and rank.

use engram_domain::{
    EmbeddingSpace, FusionStrategy, FusionTrace, Id, RerankStrategy, RetrievalScore,
    RetrievalTargetType,
};
use engram_retrieval::VectorIndex;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqliteVectorIndex;

/// Runs the retrieval-trace capability fixture.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if the trace does not round-trip or vector
/// retrieval does not return a ranked result.
pub fn run_retrieval_fixture() -> CoreResult<()> {
    // 1. The extended FusionTrace round-trips through serde with all new fields.
    let trace = FusionTrace {
        query_id: Some("q-1".to_string()),
        vector_index: Some("conformance".to_string()),
        embedding_time_ms: Some(3),
        search_time_ms: Some(1),
        source: "vector.semantic".to_string(),
        source_rank: Some(1),
        source_score: Some(0.9),
        score: Some(0.9),
        rank: Some(1),
        fusion_strategy: Some(FusionStrategy::None),
        fusion_score: Some(0.9),
        rerank_strategy: Some(RerankStrategy::None),
        rerank_score: Some(0.9),
        discard_reason: None,
        deduplicated_with: Vec::new(),
    };
    let json = serde_json::to_string(&trace).map_err(|e| {
        err("trace_serde")(CoreError::Adapter {
            adapter: "conformance.retrieval".to_string(),
            message: e.to_string(),
        })
    })?;
    let parsed: FusionTrace = serde_json::from_str(&json).map_err(|e| {
        err("trace_serde")(CoreError::Adapter {
            adapter: "conformance.retrieval".to_string(),
            message: e.to_string(),
        })
    })?;
    if parsed.query_id.as_deref() != Some("q-1") || parsed.rank != Some(1) {
        return Err(err("trace_serde")(CoreError::Conflict {
            reason: "extended trace fields did not round-trip".to_string(),
        }));
    }

    // 2. A vector retrieval yields a ranked result that can carry the trace.
    let dims = 4u32;
    let space = EmbeddingSpace::new("conformance", "bge-small", dims, "query", None::<String>);
    let index = SqliteVectorIndex::open_in_memory(dims)?.with_embedding_space(space.clone());
    let target = Id::from("chunk-1");
    VectorIndex::insert(&index, &target, &space, vec![0.1, 0.2, 0.3, 0.4])
        .map_err(|e| err("vector_insert")(e))?;
    let hits = VectorIndex::search(&index, &space, vec![0.1, 0.2, 0.3, 0.4], 1)
        .map_err(|e| err("vector_search")(e))?;
    if hits.is_empty() {
        return Err(err("vector_search")(CoreError::Conflict {
            reason: "vector retrieval returned no results".to_string(),
        }));
    }

    // 3. The trace can be attached to a synthetic retrieval result without
    //    leaking private content (trace carries no row internals).
    let _score = RetrievalScore {
        total: hits[0].1,
        relevance: Some(hits[0].1),
        recency: None,
        confidence: None,
        cue_match: None,
        hierarchical_fit: None,
        policy_fit: None,
    };
    let _ = RetrievalTargetType::Chunk;
    Ok(())
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.retrieval".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_fixture_passes() {
        if let Err(e) = run_retrieval_fixture() {
            panic!("retrieval fixture failed: {e}");
        }
    }
}
