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
use serde_json::Value as JsonValue;

use crate::{
    hash::content_hash,
    source_key::{SOURCE_PATH_KEY, STABLE_SOURCE_KEY},
};

/// The graph records produced by one extraction pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedGraph {
    pub graph: KnowledgeGraph,
    pub entities: Vec<KnowledgeEntity>,
    pub relationships: Vec<KnowledgeRelationship>,
    /// (chunk_index, entity_refs) — which entities were extracted from which
    /// chunk. Used by `extract_into` to stamp entity refs back onto chunks so
    /// Q&A can find the actual code that defines an entity.
    pub chunk_entities: Vec<(usize, Vec<EntityRef>)>,
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
        self.extract_with_calls(source, document, chunks, None)
    }

    /// Extracts a graph, optionally using pre-computed AST call edges instead of
    /// co-occurrence. When `ast_calls` is `Some`, relationships are formed from
    /// real call expressions (no false positives from comments/strings).
    pub fn extract_with_calls(
        &self,
        source: &KnowledgeSource,
        document: &SourceDocument,
        chunks: &[KnowledgeChunk],
        ast_calls: Option<&[(String, String)]>,
    ) -> CoreResult<ExtractedGraph> {
        let now = Utc::now();
        let graph_id = graph_id_for(document);

        // T4: Stamp the graph's metadata with the stable-source-key (from the
        // source's metadata, threaded there by the ingestor from the request)
        // and the document's source-relative path. Both are lifted into indexed
        // columns by the SQLite adapter on write.
        let graph_metadata = {
            let mut m = Metadata::default();
            if let Some(key) = source
                .metadata
                .as_ref()
                .and_then(|meta| meta.get(STABLE_SOURCE_KEY))
                .and_then(|v| v.as_str())
            {
                m.insert(
                    STABLE_SOURCE_KEY.to_owned(),
                    JsonValue::String(key.to_owned()),
                );
            }
            if let Some(path) = &document.path {
                m.insert(SOURCE_PATH_KEY.to_owned(), JsonValue::String(path.clone()));
            }
            if m.is_empty() { None } else { Some(m) }
        };

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
            metadata: graph_metadata,
        };

        let is_code = matches!(document.kind, SourceDocumentKind::Code);

        // (name, kind, body, chunk_index) per detected symbol, in document order.
        let mut symbols: Vec<(String, EntityKind, String, usize)> = Vec::new();
        if is_code {
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
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
                symbols.push((name, kind, chunk.text.clone(), chunk_idx));
            }
        } else {
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                let name = concept_name(&chunk.text);
                if name.is_empty() {
                    continue;
                }
                symbols.push((name, EntityKind::Concept, chunk.text.clone(), chunk_idx));
            }
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
                valid_from: None,
                valid_until: None,
                metadata: None,
            });
        }

        // Edges: prefer AST-extracted calls when available; fall back to
        // co-occurrence (comments/text mentions).
        let predicate = if is_code { "calls" } else { "mentions" };
        let mut relationships = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();

        if let Some(calls) = ast_calls {
            // AST-level calls: each (caller, callee) is a real call expression.
            for (caller, callee) in calls {
                if caller == callee {
                    continue;
                }
                if !index.contains_key(caller) {
                    continue;
                }
                if !seen.insert((caller.clone(), callee.clone())) {
                    continue;
                }
                let subject_index = index[caller];
                // Cross-file call: callee not in this document — create a
                // name-only ref. The cross-file resolver connects it by name.
                let object_ref = if let Some(&oi) = index.get(callee) {
                    entity_ref(&entities[oi])
                } else {
                    EntityRef {
                        id: None,
                        kind: None,
                        name: Some(callee.clone()),
                        aliases: Vec::new(),
                    }
                };
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, caller, callee),
                    graph_id: Some(graph_id.clone()),
                    subject: entity_ref(&entities[subject_index]),
                    predicate: "calls".to_owned(),
                    object: object_ref,
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(0.9),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                });
            }
        } else {
            // Co-occurrence fallback: name appears in body text.
            for (subject_name, _kind, body, _chunk_idx) in &symbols {
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
        }

        // Build chunk→entity mapping: stamp entity refs back onto the chunks
        // they came from so Q&A can find the actual code (not just text that
        // mentions the entity name).
        let mut chunk_entities_map: HashMap<usize, Vec<EntityRef>> = HashMap::new();
        for (name, _kind, _body, chunk_idx) in &symbols {
            if let Some(&entity_idx) = index.get(name) {
                chunk_entities_map
                    .entry(*chunk_idx)
                    .or_default()
                    .push(entity_ref(&entities[entity_idx]));
            }
        }
        let chunk_entities = chunk_entities_map.into_iter().collect::<Vec<_>>();

        // T5: Emit exactly one EntityKind::Repository node per source (keyed by
        // (scope.tenant, stable_source_key) so idempotent upserts converge across
        // documents and re-scans), plus a belongs_to relationship from this
        // document graph to the Repository node. The Repository entity has
        // graph_id = None (not file-scoped). The belongs_to edge carries the
        // document graph's graph_id so it retracts with the graph.
        let graph_name_for_bt = graph.name.clone();
        if let Some(key) = source
            .metadata
            .as_ref()
            .and_then(|meta| meta.get(STABLE_SOURCE_KEY))
            .and_then(|v| v.as_str())
        {
            let repo_id = repo_entity_id(&source.scope, key);
            let rel_id = belongs_to_rel_id(&graph_id, &repo_id);

            let mut repo_meta = Metadata::default();
            repo_meta.insert(
                STABLE_SOURCE_KEY.to_owned(),
                JsonValue::String(key.to_owned()),
            );
            entities.push(KnowledgeEntity {
                id: repo_id.clone(),
                graph_id: None, // per spec: Repository node is not file-scoped
                kind: EntityKind::Repository,
                name: key.to_owned(),
                aliases: Vec::new(),
                scope: source.scope.clone(),
                source_refs: Vec::new(),
                concept_refs: Vec::new(),
                provenance: source.provenance.clone(),
                created_at: now,
                updated_at: None,
                valid_from: None,
                valid_until: None,
                metadata: Some(repo_meta),
            });

            relationships.push(KnowledgeRelationship {
                id: rel_id,
                graph_id: Some(graph_id.clone()), // edge retracts with the document graph
                subject: EntityRef {
                    id: Some(graph_id.clone()),
                    kind: Some("graph".to_owned()),
                    name: Some(graph_name_for_bt),
                    aliases: Vec::new(),
                },
                predicate: "belongs_to".to_owned(),
                object: EntityRef {
                    id: Some(repo_id),
                    kind: Some("repository".to_owned()),
                    name: Some(key.to_owned()),
                    aliases: Vec::new(),
                },
                scope: source.scope.clone(),
                evidence: Vec::new(),
                confidence: Some(1.0),
                provenance: source.provenance.clone(),
                created_at: now,
                updated_at: None,
            });
        }

        Ok(ExtractedGraph {
            graph,
            entities,
            relationships,
            chunk_entities,
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
        name_index: Option<&mut HashMap<String, String>>,
    ) -> CoreResult<ExtractedGraph>
    where
        R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
    {
        let mut extracted = Self.extract(source, document, chunks)?;

        // Cross-file edge resolution (C1): fill name-only calls object refs
        // against the caller-maintained global name→id index.
        if let Some(index) = name_index {
            for entity in &extracted.entities {
                index.insert(entity.name.clone(), entity.id.to_string());
            }
            for rel in &mut extracted.relationships {
                if rel.predicate == "calls" && rel.object.id.is_none() {
                    if let Some(name) = &rel.object.name {
                        if let Some(id) = index.get(name) {
                            rel.object.id = Some(Id::from(id.clone()));
                        }
                    }
                }
            }
        }

        repository.put_graph(extracted.graph.clone()).await?;
        for entity in &extracted.entities {
            repository.put_entity(entity.clone()).await?;
        }
        for relationship in &extracted.relationships {
            repository.put_relationship(relationship.clone()).await?;
        }
        // Re-persist chunks with their entity refs stamped (Part A).
        for (chunk_idx, entity_refs) in &extracted.chunk_entities {
            if let Some(chunk) = chunks.get(*chunk_idx) {
                let mut updated = chunk.clone();
                updated.entities = entity_refs.clone();
                repository.put_chunk(updated).await?;
            }
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
        "struct" | "record" => EntityKind::Struct,
        "enum" => EntityKind::Enum,
        "trait" => EntityKind::Trait,
        "interface" => EntityKind::Interface,
        "type" => EntityKind::TypeAlias,
        "class" | "impl" => EntityKind::Class,
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

/// Derives a stable, deterministic `EntityId` for the per-source Repository
/// node. Keyed on the FULL scope discriminator `(tenant, subject, workspace,
/// session, environment, stable_source_key)` so that the id matches the stored
/// scope exactly — two documents of the same repo under the same scope converge
/// to one entity (idempotent upsert), while the same repo under a different
/// scope (e.g. different workspace) produces a distinct entity whose
/// `scope_allows` filter is consistent with the id.
///
/// Exposed as `pub(crate)` so the ingest reconciler can compute the same id
/// when deciding whether to delete the per-source Repository node.
pub(crate) fn repo_entity_id(scope: &Scope, key: &str) -> EntityId {
    Id::from(format!(
        "repo-{}",
        content_hash(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{key}",
            scope.tenant,
            scope.subject.as_deref().unwrap_or(""),
            scope.workspace.as_deref().unwrap_or(""),
            scope.session.as_deref().unwrap_or(""),
            scope.environment.as_deref().unwrap_or(""),
        ))
        .trim_start_matches("sha256:")
    ))
}

/// Derives a stable `RelationshipId` for the `belongs_to` edge from a document
/// graph to its Repository node. Keyed on `(graph_id, repo_entity_id)` so the
/// edge is idempotent and retracts when the graph is removed.
fn belongs_to_rel_id(graph_id: &KnowledgeGraphId, repo_entity_id: &EntityId) -> RelationshipId {
    Id::from(format!(
        "belongs-{}",
        content_hash(format!("{graph_id}\u{1f}{repo_entity_id}")).trim_start_matches("sha256:")
    ))
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
