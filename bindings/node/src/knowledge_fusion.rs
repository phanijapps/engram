//! Graph fusion and candidate retrieval operations.
//!
//! Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
//! candidates for requests, and reciprocal-rank fusion of candidate lists.

use engram_domain::RetrievalRequest;
use engram_domain::RetrievalResult;
use engram_retrieval::{
    DEFAULT_RRF_K, ReciprocalFusionConfig, ReciprocalRankFusion, RetrievalFusion, RetrievalIndex,
};
use engram_store_knowledge_sqlite::{GraphCandidateSource, GraphRetrievalIndex, SqlKnowledgeStore};
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, to_napi_error};

/// Retrieval-composition seam (RFC-0005): graph-ranked Entity/Chunk
/// candidates for a request, as `RetrievalResult` JSON tagged
/// `source = "graph"`, ready to RRF-fuse with vector candidates.
pub fn graph_candidates_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let request = decode::<RetrievalRequest>(&request_json)?;
    let source: Arc<dyn GraphCandidateSource> = store.clone();
    let index = GraphRetrievalIndex::new(source);
    let results = block_on(index.retrieve_candidates(&request)).map_err(to_napi_error)?;
    encode(&results)
}

/// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of
/// candidate lists (graph + vector) into one ranked list. Configurable
/// strength (`k`, per-source `weights`) with defaults when omitted.
pub fn fuse_rrf_json(_store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let request: RetrievalRequest = serde_json::from_value(value["request"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let candidates: Vec<RetrievalResult> = serde_json::from_value(value["candidates"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let k = value["k"]
        .as_u64()
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_RRF_K);
    let default_weight = value["defaultWeight"]
        .as_f64()
        .map(|f| f as f32)
        .unwrap_or(1.0);
    let weights: std::collections::BTreeMap<String, f32> = value
        .get("weights")
        .and_then(|w| w.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                .collect()
        })
        .unwrap_or_default();
    let config = ReciprocalFusionConfig::new(k, default_weight, weights).map_err(to_napi_error)?;
    let fused = ReciprocalRankFusion::new(config)
        .fuse(&request, candidates)
        .map_err(to_napi_error)?;
    encode(&fused)
}

/// Retrieval-composition seam (RFC-0005): reciprocal-rank fusion of ranked
/// id lists (e.g. graph chunk ids + vector chunk ids) into one fused order.
/// Lightweight alternative to `fuseRrfJson` for callers that have ranked id
/// lists, not full `RetrievalResult`s — the demo uses this to fuse graph +
/// vector chunk orders without marshaling Provenance/Policy per candidate.
/// The formula mirrors `ReciprocalRankFusion` (1/(k + rank)); the canonical,
/// tested impl lives in `engram-retrieval`.
pub fn fuse_rrf_ids_json(_store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let lists: Vec<Vec<String>> = serde_json::from_value(value["lists"].clone())
        .map_err(|e| Error::from_reason(e.to_string()))?;
    let k = value["k"]
        .as_u64()
        .map(|n| n as u32)
        .unwrap_or(DEFAULT_RRF_K) as f32;
    let limit = value["limit"].as_u64().map(|n| n as usize);
    // score(id) = Σ over lists of 1/(k + rank_in_list); first-seen order for
    // a stable tiebreak. An id in two lists is boosted (cross-source consensus).
    let mut scores: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for list in &lists {
        for (rank, id) in list.iter().enumerate() {
            let contribution = 1.0 / (k + (rank + 1) as f32);
            if scores.insert(id.clone(), 0.0).is_none() {
                order.push(id.clone());
            }
            if let Some(s) = scores.get_mut(id) {
                *s += contribution;
            }
        }
    }
    order.sort_by(|a, b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(limit) = limit {
        order.truncate(limit);
    }
    encode(&order)
}
