//! Caller-facing request models for knowledge ingestion.
//!
//! These request structs are the narrow boundary between source readers and the
//! deterministic ingestor. They carry caller-provided metadata, policy, scope,
//! and actor information without introducing filesystem or Git dependencies.

use engram_domain::{Actor, Policy, Scope, SourceDocumentKind, SourceKind, SourceLocation};
use serde::{Deserialize, Serialize};

/// Optional document metadata supplied by a source reader or caller.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct DocumentIngestRequest {
    pub source_kind: SourceKind,
    pub source_name: String,
    pub scope: Scope,
    pub document_kind: SourceDocumentKind,
    pub document: DocumentMetadata,
    pub text: String,
    pub policy: Policy,
    pub actor: Actor,
}
