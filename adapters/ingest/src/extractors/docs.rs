//! Prose concept extraction from text chunks.
//!
//! This module extracts concept entities from prose documents (markdown, text,
//! documentation) by deriving concept names from chunk text. It builds
//! `mentions` relationships based on co-occurrence in chunk text.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use engram_domain::*;
use engram_knowledge::CoreResult;

use crate::chunker::{Chunker, PlainTextChunker, PlainTextChunkerOptions};
use crate::extractors::SourceExtractor;
use crate::hash::content_hash;

/// Docs extractor that produces concept entities from prose chunks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocsExtractor;

impl DocsExtractor {
    /// Creates a new docs extractor.
    pub fn new() -> Self {
        Self
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
}

impl SourceExtractor for DocsExtractor {
    fn extract(
        &self,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
        scope: &Scope,
    ) -> CoreResult<(Vec<KnowledgeEntity>, Vec<KnowledgeRelationship>)> {
        let now = Utc::now();
        let graph_id = graph_id_for(document);

        // (name, kind, body, chunk_index) per detected concept, in document order.
        let mut concepts: Vec<(String, EntityKind, String, usize)> = Vec::new();
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let name = Self::concept_name(&chunk.text);
            if name.is_empty() {
                continue;
            }
            concepts.push((name, EntityKind::Concept, chunk.text.clone(), chunk_idx));
        }

        // Dedupe by name (first wins), build entities + a name->index map.
        let mut entities: Vec<KnowledgeEntity> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (name, kind, _body, _chunk_idx) in &concepts {
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

        // Edges: co-occurrence (name appears in body text).
        let mut relationships = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();

        for (subject_name, _kind, body, _chunk_idx) in &concepts {
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
                    predicate: "mentions".to_owned(),
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
        Ok(Box::new(PlainTextChunker::new(
            PlainTextChunkerOptions::default(),
        )?))
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn entity_ref(entity: &KnowledgeEntity) -> EntityRef {
    EntityRef {
        id: Some(entity.id.clone()),
        kind: Some("concept".to_owned()),
        name: Some(entity.name.clone()),
        aliases: Vec::new(),
    }
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
    fn concept_name_from_markdown_heading() {
        assert_eq!(
            DocsExtractor::concept_name("# Introduction\nThis is the intro"),
            "Introduction".to_owned()
        );
    }

    #[test]
    fn concept_name_from_plain_text() {
        assert_eq!(
            DocsExtractor::concept_name("This is a concept about memory"),
            "This is a concept about memory".to_owned()
        );
    }

    #[test]
    fn concept_name_truncates_long_lines() {
        let long = "a".repeat(100);
        let result = DocsExtractor::concept_name(&long);
        assert!(result.len() <= 48);
    }
}
