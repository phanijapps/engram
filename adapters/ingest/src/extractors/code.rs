//! Code symbol extraction using tree-sitter anchors.
//!
//! This module extracts function and class entities from code chunks by parsing
//! tree-sitter anchors. It builds `calls` relationships from AST call expressions
//! (when available) or falls back to co-occurrence in symbol bodies.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::CoreResult;

use crate::chunker::Chunker;
use crate::extractors::SourceExtractor;
use crate::hash::content_hash;
use crate::tree_sitter_chunker::TreeSitterChunker;

/// Code extractor that produces function/class entities from tree-sitter anchors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodeExtractor;

impl CodeExtractor {
    /// Creates a new code extractor.
    pub fn new() -> Self {
        Self
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
}

impl SourceExtractor for CodeExtractor {
    fn extract(
        &self,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
        scope: &Scope,
    ) -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>)> {
        let now = Utc::now();
        let graph_id = graph_id_for(document);

        // (name, kind, body, chunk_index) per detected symbol, in document order.
        let mut symbols: Vec<(String, EntityKind, String, usize)> = Vec::new();
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let Some(anchor) = chunk
                .location
                .as_ref()
                .and_then(|location| location.anchor.as_deref())
            else {
                continue;
            };
            let Some((kind, name)) = Self::parse_symbol(anchor) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            symbols.push((name, kind, chunk.text.clone(), chunk_idx));
        }

        // Dedupe by name (first wins), build entities + a name->index map.
        let mut entities: Vec<KnowledgeEntity> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (name, kind, _body, _chunk_idx) in &symbols {
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
                scope: scope.clone(),
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
                provenance: document.provenance.clone(),
                created_at: now,
                updated_at: None,
                metadata: None,
            });
        }

        // Edges: co-occurrence fallback (name appears in body text).
        // AST-level calls are handled by the caller via `extract_with_calls`.
        let mut relationships = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();

        for (subject_name, _kind, body, _chunk_idx) in &symbols {
            let Some(&subject_index) = index.get(subject_name) else {
                continue;
            };
            for object_name in index.keys() {
                if object_name == subject_name || !Self::mentions(body, object_name) {
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
                    predicate: "calls".to_owned(),
                    object: entity_ref(&entities[object_index]),
                    scope: scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(0.5),
                    provenance: document.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                });
            }
        }

        Ok((entities, relationships))
    }

    fn select_chunker(&self) -> CoreResult<Box<dyn Chunker>> {
        Ok(Box::new(TreeSitterChunker::new()?))
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_symbol_parses_function() {
        assert_eq!(
            CodeExtractor::parse_symbol("fn remember"),
            Some((EntityKind::Function, "remember".to_owned()))
        );
    }

    #[test]
    fn parse_symbol_parses_class() {
        assert_eq!(
            CodeExtractor::parse_symbol("struct MemoryRecord"),
            Some((EntityKind::Class, "MemoryRecord".to_owned()))
        );
    }

    #[test]
    fn parse_symbol_ignores_non_declarations() {
        assert_eq!(CodeExtractor::parse_symbol("file"), None);
    }

    #[test]
    fn mentions_is_multibyte_safe() {
        // Regression: the scan used to advance one byte past a match, which lands
        // inside a multi-byte UTF-8 char and panics.
        assert!(CodeExtractor::mentions("x│ token │", "│"));
        // Scanning a body full of 3-byte box chars for an absent name must walk
        // the whole body without panicking.
        let body = "┌───────────┼───────────┐\n▼           ▼           ▼\n";
        assert!(!CodeExtractor::mentions(body, "│")); // body has corners/cross/down-arrow, no vertical bar
        assert!(CodeExtractor::mentions(body, "▼")); // present
    }
}
