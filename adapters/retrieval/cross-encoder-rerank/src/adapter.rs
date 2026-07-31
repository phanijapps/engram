//! Adapter: CrossEncoderReranker → RetrievalReranker port.

use engram_domain::{RetrievalRequest, RetrievalResult};
use engram_retrieval::RetrievalReranker;
use engram_runtime::CoreResult;

use crate::rerank::CrossEncoderReranker;

/// Wraps a `CrossEncoderReranker` to implement the `RetrievalReranker` port.
pub struct CrossEncoderRerankerAdapter {
    inner: CrossEncoderReranker,
}

impl CrossEncoderRerankerAdapter {
    pub fn new(inner: CrossEncoderReranker) -> Self {
        Self { inner }
    }
}

impl RetrievalReranker for CrossEncoderRerankerAdapter {
    fn rerank(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request.limit.map(|l| l as usize);
        self.inner.rerank(&request.query, candidates, limit)
    }
}
