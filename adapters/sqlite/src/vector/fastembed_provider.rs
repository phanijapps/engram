//! Compatibility name and SQLite query-port delegation for neutral inference.
pub use engram_embedding_fastembed::FastEmbedBgeSmallQueryProvider;

impl super::VectorQueryProvider for FastEmbedBgeSmallQueryProvider {
    fn query_vector(
        &self,
        request: &engram_domain::RetrievalRequest,
    ) -> engram_runtime::CoreResult<Vec<f32>> {
        self.embed_query(&request.query)
    }
}
