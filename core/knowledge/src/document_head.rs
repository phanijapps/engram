//! Current-document authority, separate from retained canonical/history records.
use async_trait::async_trait;
use engram_domain::{
    ChunkId, DocumentHead, DocumentHeadKey, KnowledgeChunk, KnowledgeSource, Scope, SourceDocument,
};
use engram_runtime::CoreResult;

/// Canonical writes fenced by one updating head in the owning knowledge cell.
pub enum DocumentHeadWrite {
    Source(KnowledgeSource),
    Document(SourceDocument),
    Chunk(KnowledgeChunk),
    Graph(engram_domain::KnowledgeGraph),
    Entity(engram_domain::KnowledgeEntity),
    Relationship(engram_domain::KnowledgeRelationship),
    UnresolvedReference(engram_domain::UnresolvedReference),
}

/// Request-time authorization is independent of cached projection contents.
pub fn document_head_retrieval_authorized(
    head: &DocumentHead,
    scope: &Scope,
    requester: &engram_domain::Requester,
    now: engram_domain::Timestamp,
) -> bool {
    head.state == engram_domain::CodeSourceState::Active
        && &head.key.scope == scope
        && head
            .policy
            .allowed_uses
            .contains(&engram_domain::AllowedUse::Retrieval)
        && head.policy.expires_at.is_none_or(|expiry| expiry > now)
        && (head.policy.visibility != engram_domain::Visibility::Private
            || requester.actor.id == head.provenance.actor.id)
}

#[async_trait]
pub trait DocumentHeadRepository: Send + Sync {
    /// Ownership existence, independent of eligibility. This distinguishes
    /// legacy unmanaged data from an owned but unavailable projection.
    async fn is_managed_chunk(&self, id: &ChunkId) -> CoreResult<bool>;
    async fn get_document_head(&self, key: &DocumentHeadKey) -> CoreResult<Option<DocumentHead>>;
    async fn list_document_heads(
        &self,
        scope: &Scope,
        repository: &str,
    ) -> CoreResult<Vec<DocumentHead>>;
    /// Compare-and-set before refreshing; an old attempt may never be reused.
    async fn begin_document_head(
        &self,
        head: DocumentHead,
        expected_attempt: Option<&str>,
    ) -> CoreResult<()>;
    async fn write_document_record(
        &self,
        key: &DocumentHeadKey,
        attempt: &str,
        payload: DocumentHeadWrite,
    ) -> CoreResult<()>;
    /// Verifies the complete stored inventory and immutable lease before activation.
    async fn activate_document_head(&self, head: DocumentHead) -> CoreResult<()>;
    async fn fail_document_head(&self, key: &DocumentHeadKey, attempt: &str) -> CoreResult<()>;
    /// Resolves retained/stale projection identities through current exact scope.
    /// None means unverified, obsolete or unavailable, never "latest timestamp".
    async fn current_document_for_chunk(
        &self,
        id: &ChunkId,
        scope: &Scope,
    ) -> CoreResult<Option<DocumentHead>>;
}
