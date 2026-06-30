//! External retrieval index composition for the in-memory adapter.
//!
//! This module calls injected `RetrievalIndex` sources and translates their
//! non-fatal failures into contract-level source failure records. Concrete
//! vector, graph, SQL, or provider-backed behavior stays in adapter crates.

use std::sync::Arc;

use engram_core::RetrievalIndex;
use engram_domain::{
    RetrievalRequest, RetrievalResult, RetrievalSourceFailure, SourceFailureSeverity,
};

/// Collects candidates from injected retrieval indexes without failing local retrieval.
///
/// A failed index degrades the response but does not abort the request because
/// in-memory memory, knowledge, belief, and hierarchy sources may still return
/// useful context. Infrastructure-specific details remain in the error message
/// while callers get a portable `RetrievalSourceFailure`.
pub(crate) async fn external_candidates(
    indexes: &[Arc<dyn RetrievalIndex>],
    request: &RetrievalRequest,
) -> (Vec<RetrievalResult>, Vec<RetrievalSourceFailure>) {
    let mut candidates = Vec::new();
    let mut failures = Vec::new();

    for (index, retrieval_index) in indexes.iter().enumerate() {
        match retrieval_index.retrieve_candidates(request).await {
            Ok(mut results) => candidates.append(&mut results),
            Err(error) => failures.push(RetrievalSourceFailure {
                source: format!("retrieval_index.{}", index + 1),
                mode: request.modes.first().cloned(),
                severity: SourceFailureSeverity::Warning,
                reason: "external_retrieval_index_failed".to_owned(),
                message: Some(error.to_string()),
                degraded: true,
            }),
        }
    }

    (candidates, failures)
}
