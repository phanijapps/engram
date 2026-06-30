//! Deterministic knowledge-graph extraction from ingested chunks.
//!
//! Produces `KnowledgeEntity` + `KnowledgeRelationship` records from chunk
//! anchors and text. Code symbols (from `CodeSymbolChunker` anchors) become
//! `Function`/`Class` entities with `calls` edges inferred from name occurrences
//! in symbol bodies; prose chunks become `Concept` entities with `mentions`
//! edges. No model calls — deterministic and testable. This is demo-grade
//! extraction; a later model-backed extractor can sit behind the same ports.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};

use crate::hash::content_hash;

/// The graph records produced by one extraction pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedGraph {
    pub graph: KnowledgeGraph,
    pub entities: Vec<KnowledgeEntity>,
    pub relationships: Vec<KnowledgeRelationship>,
}

/// Deterministic extractor that turns ingested chunks into a scoped graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphExtractor;

impl GraphExtractor {
    /// Creates a new extractor.
    pub fn new() -> Self {
        Self
    }

    /// Extracts a graph from ingested chunks. Pure: it builds records but does
    /// not persist them. Call `extract_into` to persist through a repository.
    pub fn extract(
        &self,
        source: &KnowledgeSource,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
    ) -> CoreResult<ExtractedGraph> {
        let now = Utc::now();
        let graph_id = graph_id_for(document);
        let graph = KnowledgeGraph {
            id: graph_id.clone(),
            scope: source.scope.clone(),
            name: document
                .title
                .clone()
                .unwrap_or_else(|| source.name.clone()),
            uri: document.uri.clone(),
            version: document.version.clone(),
            ontology_refs: Vec::new(),
            policy: source.policy.clone(),
            provenance: source.provenance.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        };

        let is_code = matches!(document.kind, SourceDocumentKind::Code);

        // (name, kind, body) per detected symbol, in document order.
        let mut symbols: Vec<(String, EntityKind, String)> = Vec::new();
        if is_code {
            for chunk in chunks {
                let Some(anchor) = chunk
                    .location
                    .as_ref()
                    .and_then(|location| location.anchor.as_deref())
                else {
                    continue;
                };
                let Some((kind, name)) = parse_symbol(anchor) else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                symbols.push((name, kind, chunk.text.clone()));
            }
        } else {
            for chunk in chunks {
                let name = concept_name(&chunk.text);
                if name.is_empty() {
                    continue;
                }
                symbols.push((name, EntityKind::Concept, chunk.text.clone()));
            }
        }

        // Dedupe by name (first wins), build entities + a name->index map.
        let mut entities: Vec<KnowledgeEntity> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (name, kind, _body) in &symbols {
            if index.contains_key(name) {
                continue;
            }
            index.insert(name.clone(), entities.len());
            entities.push(KnowledgeEntity {
                id: entity_id(&graph_id, name),
                graph_id: Some(graph_id.clone()),
                kind: kind.clone(),
                name: name.clone(),
                aliases: Vec::new(),
                scope: source.scope.clone(),
                source_refs: vec![EvidenceRef {
                    target_type: EvidenceTargetType::Document,
                    target_id: Some(document.id.to_string()),
                    uri: None,
                    quote: None,
                    location: Some(SourceLocation {
                        path: document.path.clone(),
                        start_line: None,
                        end_line: None,
                        start_offset: None,
                        end_offset: None,
                        anchor: None,
                    }),
                }],
                concept_refs: Vec::new(),
                provenance: source.provenance.clone(),
                created_at: now,
                updated_at: None,
                metadata: None,
            });
        }

        // Edges from name co-occurrence in symbol bodies (calls / mentions).
        let predicate = if is_code { "calls" } else { "mentions" };
        let mut relationships = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for (subject_name, _kind, body) in &symbols {
            let Some(&subject_index) = index.get(subject_name) else {
                continue;
            };
            for object_name in index.keys() {
                if object_name == subject_name || !mentions(body, object_name) {
                    continue;
                }
                if !seen.insert((subject_name.clone(), object_name.clone())) {
                    continue;
                }
                let object_index = index[object_name];
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, subject_name, object_name),
                    graph_id: Some(graph_id.clone()),
                    subject: entity_ref(&entities[subject_index]),
                    predicate: predicate.to_owned(),
                    object: entity_ref(&entities[object_index]),
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(0.5),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                });
            }
        }

        Ok(ExtractedGraph {
            graph,
            entities,
            relationships,
        })
    }

    /// Extracts a graph and persists it (graph + entities + relationships)
    /// through the supplied repository.
    pub async fn extract_into<R>(
        &self,
        repository: &R,
        source: &KnowledgeSource,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
    ) -> CoreResult<ExtractedGraph>
    where
        R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
    {
        let extracted = Self.extract(source, document, chunks)?;
        repository.put_graph(extracted.graph.clone()).await?;
        for entity in &extracted.entities {
            repository.put_entity(entity.clone()).await?;
        }
        for relationship in &extracted.relationships {
            repository.put_relationship(relationship.clone()).await?;
        }
        Ok(extracted)
    }
}

/// Parses a code-symbol anchor (`"fn remember"`, `"struct MemoryRecord"`) into a
/// kind + name. Returns `None` for non-declaration anchors (e.g. `"file"`).
fn parse_symbol(anchor: &str) -> Option<(EntityKind, String)> {
    let mut parts = anchor.splitn(2, ' ');
    let keyword = parts.next()?.trim();
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }
    let kind = match keyword {
        "fn" | "function" | "def" | "func" => EntityKind::Function,
        "struct" | "enum" | "trait" | "impl" | "class" | "interface" | "record" | "type" => {
            EntityKind::Class
        }
        _ => return None,
    };
    Some((kind, name.to_owned()))
}

/// Derives a short, human-readable concept name from the first line of a prose
/// chunk (used for non-code documents).
fn concept_name(text: &str) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    let stripped = first.trim_start_matches('#').trim();
    let collapsed: String = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(48).collect()
}

/// Word-boundary occurrence check so `File` does not match inside `Filesystem`.
fn mentions(body: &str, name: &str) -> bool {
    let bytes = body.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() || needle.len() > bytes.len() {
        return false;
    }
    let mut from = 0;
    while let Some(relative) = body[from..].find(name) {
        let start = from + relative;
        let end = start + name.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        // Advance past the entire match (a char boundary) — advancing by one
        // byte could land inside a multi-byte UTF-8 char and panic the slice.
        from = end;
        if from >= body.len() {
            break;
        }
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::mentions;

    #[test]
    fn mentions_is_multibyte_safe() {
        // Regression: the scan used to advance one byte past a match (`from =
        // start + 1`), which lands inside a multi-byte UTF-8 char and panics the
        // `body[from..]` slice. The agentzero repo (box-drawing art) hit this.
        // A name starting with a 3-byte char, matched first at a non-word-boundary,
        // forces the scan to advance — the old code panicked here.
        assert!(mentions("x│ token │", "│"));
        // Scanning a body full of 3-byte box chars for an absent name must walk
        // the whole body without panicking.
        let body = "┌───────────┼───────────┐\n▼           ▼           ▼\n";
        assert!(!mentions(body, "│")); // body has corners/cross/down-arrow, no vertical bar
        assert!(mentions(body, "▼")); // present
    }
}

fn entity_ref(entity: &KnowledgeEntity) -> EntityRef {
    EntityRef {
        id: Some(entity.id.clone()),
        kind: Some(kind_label(entity.kind.clone())),
        name: Some(entity.name.clone()),
        aliases: Vec::new(),
    }
}

fn kind_label(kind: EntityKind) -> String {
    match kind {
        EntityKind::Function => "function",
        EntityKind::Class => "class",
        EntityKind::Concept => "concept",
        _ => "entity",
    }
    .to_owned()
}

fn graph_id_for(document: &SourceDocument) -> KnowledgeGraphId {
    Id::from(format!(
        "graph-{}",
        content_hash(document.id.as_str()).trim_start_matches("sha256:")
    ))
}

fn entity_id(graph_id: &KnowledgeGraphId, name: &str) -> EntityId {
    Id::from(format!(
        "entity-{}",
        content_hash(format!("{graph_id}\u{1f}{name}")).trim_start_matches("sha256:")
    ))
}

fn relationship_id(graph_id: &KnowledgeGraphId, subject: &str, object: &str) -> RelationshipId {
    Id::from(format!(
        "rel-{}",
        content_hash(format!("{graph_id}\u{1f}{subject}\u{1f}{object}"))
            .trim_start_matches("sha256:")
    ))
}
