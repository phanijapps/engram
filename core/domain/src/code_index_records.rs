//! Knowledge-owned derived index records; canonical code roles stay unchanged.
use crate::{
    ChunkId, CodeByteRange, CodeContextFact, CodeFileKey, CodeSourceCategory, CodeSourceState,
    CodeTokenCoverage, DocumentId, Policy, Provenance, Scope,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeEmbeddingSegment {
    pub id: String,
    pub parent_chunk_id: ChunkId,
    pub parent_unit_id: String,
    pub file: CodeFileKey,
    pub generation: String,
    pub attempt_id: String,
    pub source_hash: String,
    pub representation_id: String,
    pub ranges: Vec<CodeByteRange>,
    pub continuation: bool,
    pub source_category: CodeSourceCategory,
    pub context: Vec<CodeContextFact>,
    pub embedding_input: CodeTokenCoverage,
}

/// Exact source identity; `source_key` is opaque, including for non-file readers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentHeadKey {
    pub scope: Scope,
    pub repository: String,
    pub source_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentHead {
    pub key: DocumentHeadKey,
    pub generation: String,
    pub attempt_id: String,
    pub state: CodeSourceState,
    pub source_hash: String,
    #[serde(deserialize_with = "crate::code_evidence::required_nullable")]
    pub document_id: Option<DocumentId>,
    pub chunk_ids: Vec<ChunkId>,
    pub policy: Policy,
    pub provenance: Provenance,
}

/// Separate from canonical source inventory: no new CodeSourceRecordKind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSegmentInventory {
    pub file: CodeFileKey,
    pub generation: String,
    pub attempt_id: String,
    pub representation_id: String,
    /// Deterministic order: canonical parent inventory order, then ascending
    /// first source byte per parent; terminal continuations follow their range.
    pub segment_ids: Vec<String>,
    pub state: CodeSourceState,
}
