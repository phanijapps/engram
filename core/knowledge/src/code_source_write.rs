//! Typed payloads for writes fenced by a per-file source attempt.
use engram_domain::*;

/// The payload is a domain record, never SQL or an infrastructure callback.
#[derive(Debug, Clone)]
// One consumed write payload at the port boundary. Keep owned domain values
// directly; boxing each variant would add allocation to every cell write.
#[allow(clippy::large_enum_variant)]
pub enum CodeSourceWrite {
    UnresolvedReference(UnresolvedReference),
    Source(KnowledgeSource),
    Document(SourceDocument),
    Chunk(KnowledgeChunk),
    Graph(KnowledgeGraph),
    Entity(KnowledgeEntity),
    Relationship(KnowledgeRelationship),
}
impl CodeSourceWrite {
    pub fn record(&self) -> CodeSourceRecord {
        let (kind, id) = match self {
            Self::UnresolvedReference(v) => (CodeSourceRecordKind::UnresolvedReference, &v.id),
            Self::Source(v) => (CodeSourceRecordKind::Source, &v.id),
            Self::Document(v) => (CodeSourceRecordKind::Document, &v.id),
            Self::Chunk(v) => (CodeSourceRecordKind::Chunk, &v.id),
            Self::Graph(v) => (CodeSourceRecordKind::Graph, &v.id),
            Self::Entity(v) => (CodeSourceRecordKind::Entity, &v.id),
            Self::Relationship(v) => (CodeSourceRecordKind::Relationship, &v.id),
        };
        CodeSourceRecord {
            kind,
            id: id.to_string(),
        }
    }
}
