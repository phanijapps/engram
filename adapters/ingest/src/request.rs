//! Caller-facing request models for knowledge ingestion.
//!
//! These request structs are the narrow boundary between source readers and the
//! deterministic ingestor. They carry caller-provided metadata, policy, scope,
//! and actor information without introducing filesystem or Git dependencies.

use engram_domain::{Actor, Policy, Scope, SourceDocumentKind, SourceKind, SourceLocation};
use serde::{Deserialize, Serialize};

/// Optional document metadata supplied by a source reader or caller.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadata {
    pub uri: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub mime_type: Option<String>,
    pub language: Option<String>,
    pub version: Option<String>,
    pub location: Option<SourceLocation>,
}

/// Text document input for the first deterministic ingestion slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentIngestRequest {
    pub source_kind: SourceKind,
    pub source_name: String,
    pub scope: Scope,
    pub document_kind: SourceDocumentKind,
    pub document: DocumentMetadata,
    pub text: String,
    pub policy: Policy,
    pub actor: Actor,
    /// SHA-free stable identity key for the source (e.g. `host/org/repo` for a
    /// git remote). Derived by the scanner when a git remote is detected; `None`
    /// for sources that do not carry a remote. Carried through to
    /// `KnowledgeSource.metadata` so the extractor can stamp each
    /// `KnowledgeGraph` and emit the per-source `EntityKind::Repository` node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_source_key: Option<String>,
}
