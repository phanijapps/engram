//! Portable source-coverage and per-file generation contracts (RFC-0021).
//!
//! These values carry source identity and byte coverage without parser objects,
//! storage locators, SQL, model providers, or inferred business semantics.

use serde::{Deserialize, Serialize};

use crate::{ChunkId, DocumentId, Policy, Provenance, Requester, Scope};

/// A file within a repository and an exact scope; repository identity alone is
/// deliberately insufficient for a source generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFileKey {
    pub scope: Scope,
    pub repository: String,
    pub path: String,
}

impl CodeFileKey {
    /// Rejects ambiguous/traversing file identities. This does not read files.
    pub fn is_valid(&self) -> bool {
        [
            &self.scope.subject,
            &self.scope.workspace,
            &self.scope.session,
            &self.scope.environment,
        ]
        .iter()
        .all(|v| v.as_ref().is_none_or(|s| !s.is_empty()))
            && !self.scope.tenant.is_empty()
            && !self.repository.is_empty()
            && !self.path.is_empty()
            && !self.path.starts_with('/')
            && !self.path.contains('\\')
            && self.path.split('/').all(|p| !matches!(p, "" | "." | ".."))
    }
}

/// Zero-based, end-exclusive UTF-8 byte range; unlike legacy SourceLocation,
/// the unit is explicit and reversed ranges fail during deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ByteRangeWire")]
pub struct CodeByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ByteRangeWire {
    start: u64,
    end: u64,
}

impl TryFrom<ByteRangeWire> for CodeByteRange {
    type Error = &'static str;
    fn try_from(value: ByteRangeWire) -> Result<Self, Self::Error> {
        if value.end < value.start {
            return Err("source byte range is reversed");
        }
        Ok(Self {
            start: value.start,
            end: value.end,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeChunkRole {
    SourceSnapshot,
    Symbol,
    ContainerContext,
    Residual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeParseQuality {
    Clean,
    Recovered,
    Fallback,
    Failed,
}

/// Exact coverage and generation association for a source-derived chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeChunkDescriptor {
    pub role: CodeChunkRole,
    pub file: CodeFileKey,
    #[serde(deserialize_with = "nonempty_string")]
    pub generation: String,
    #[serde(deserialize_with = "nonempty_string")]
    pub source_hash: String,
    #[serde(deserialize_with = "nonempty_string")]
    pub fingerprint: String,
    #[serde(deserialize_with = "nonempty_ranges")]
    pub ranges: Vec<CodeByteRange>,
    /// Text inserted between ordered source slices; it is not source coverage.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slice_separator: String,
    /// Full declaration span, distinct from rendered container context slices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_range: Option<CodeByteRange>,
    pub complete: bool,
    pub parse_quality: CodeParseQuality,
    #[serde(default)]
    pub unsupported_features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_unit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSourceState {
    Updating,
    Incomplete,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSourceRecordKind {
    UnresolvedReference,
    Source,
    Chunk,
    Entity,
    Relationship,
    Graph,
    Document,
}

/// One generation's contribution to a stored record (shared records may have
/// several independent source owners).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceRecord {
    pub kind: CodeSourceRecordKind,
    pub id: String,
}

/// Authoritative source manifest. An attempt ID fences superseded writers even
/// when an ABA sequence returns to the same content-derived generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceGeneration {
    pub file: CodeFileKey,
    #[serde(deserialize_with = "nonempty_string")]
    pub generation: String,
    #[serde(deserialize_with = "nonempty_string")]
    pub attempt_id: String,
    #[serde(deserialize_with = "nonempty_string")]
    pub source_hash: String,
    #[serde(deserialize_with = "nonempty_string")]
    pub fingerprint: String,
    pub state: CodeSourceState,
    pub byte_length: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default)]
    pub snapshot_ids: Vec<ChunkId>,
    #[serde(default)]
    pub records: Vec<CodeSourceRecord>,
    pub policy: Policy,
    pub provenance: Provenance,
}

/// Explicit source access, distinct from normal recall and model egress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceReadRequest {
    pub file: CodeFileKey,
    pub requester: Requester,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    #[serde(default = "default_read_limit")]
    pub max_bytes: u64,
}

fn default_read_limit() -> u64 {
    1024 * 1024
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceReadResult {
    #[serde(default)]
    pub coverage: Vec<CodeChunkDescriptor>,
    pub manifest: CodeSourceGeneration,
    pub text: String,
}

fn nonempty_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom(
            "source identity must not be empty",
        ));
    }
    Ok(value)
}

fn nonempty_ranges<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CodeByteRange>, D::Error> {
    let ranges = Vec::<CodeByteRange>::deserialize(deserializer)?;
    if ranges.is_empty() {
        return Err(serde::de::Error::custom(
            "source coverage requires at least one range",
        ));
    }
    Ok(ranges)
}

/// Self-contained source-transfer family in the existing ImportData envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceBundle {
    pub version: u32,
    pub manifest: CodeSourceGeneration,
    pub sources: Vec<crate::KnowledgeSource>,
    pub documents: Vec<crate::SourceDocument>,
    pub chunks: Vec<crate::KnowledgeChunk>,
    pub graphs: Vec<crate::KnowledgeGraph>,
    pub entities: Vec<crate::KnowledgeEntity>,
    pub relationships: Vec<crate::KnowledgeRelationship>,
    pub unresolved_references: Vec<crate::UnresolvedReference>,
    #[serde(default)]
    pub context_entities: Vec<crate::KnowledgeEntity>,
}

/// Source tools accept this query, without principal or scope claims.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceQuery {
    pub repository: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    #[serde(default = "default_read_limit")]
    pub max_bytes: u64,
}
/// Created by a trusted embedding host or transport launch configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSourceContext {
    pub scope: Scope,
    pub requester: Requester,
    pub reader_version: u32,
}
impl CodeSourceContext {
    pub fn request(&self, query: CodeSourceQuery) -> CodeSourceReadRequest {
        CodeSourceReadRequest {
            file: CodeFileKey {
                scope: self.scope.clone(),
                repository: query.repository,
                path: query.path,
            },
            requester: self.requester.clone(),
            generation: query.generation,
            max_bytes: query.max_bytes,
        }
    }
}
