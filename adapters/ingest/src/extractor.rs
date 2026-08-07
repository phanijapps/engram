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
    source_key::{DOCUMENT_ID_KEY, SOURCE_PATH_KEY, STABLE_SOURCE_KEY},
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
            // Stamp the document id so the reconcile path can cascade-delete
            // this document's chunks + embeddings + the document itself when
            // the file is re-ingested (knowledge-source-retraction). graph_id
            // is a non-reversible hash of document_id, so the id must be
            // carried explicitly.
            m.insert(
                DOCUMENT_ID_KEY.to_owned(),
                JsonValue::String(document.id.to_string()),
            );
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

        // {repo} identity component = the stable-source-key (RFC-0020); the git
        // remote + revision are metadata properties (Phase 4), not identity.
        let repo_key = source
            .metadata
            .as_ref()
            .and_then(|meta| meta.get(STABLE_SOURCE_KEY))
            .and_then(|v| v.as_str());
        let doc_path = document.path.as_deref();

        // (qualified_name, bare_name, kind, body, chunk_index) per detected
        // symbol, in document order. Code entities carry a qualified identity
        // `{repo}/{path}::{bare_name}`; the bare name is retained for body
        // matching (co-occurrence) and AST-callee resolution.
        let mut symbols: Vec<(String, String, EntityKind, String, usize)> = Vec::new();
        if is_code {
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                let Some(anchor) = chunk
                    .location
                    .as_ref()
                    .and_then(|location| location.anchor.as_deref())
                else {
                    continue;
                };
                let Some((kind, bare)) = parse_symbol(anchor) else {
                    continue;
                };
                if bare.is_empty() || is_noise_symbol(&bare) {
                    continue;
                }
                let qualified = qualified_symbol_name(repo_key, doc_path, &bare);
                symbols.push((qualified, bare, kind, chunk.text.clone(), chunk_idx));
            }
        } else {
            // RFC-0020 T3: non-code documents emit NO graph entities — the naive
            // heading-as-node rule is gone. Documents are chunks-only at ingest;
            // the LLM `extract-knowledge` op produces the concept sub-graph. The
            // graph record above is created unconditionally so `listGraphs` can
            // still discover documents (the extract-knowledge op relies on this).
        }

        // Bare→qualified map (first wins) for resolving AST callers/callees,
        // which treesitter emits as bare names, against the qualified entities.
        let mut bare_to_qualified: HashMap<String, String> = HashMap::new();
        for (qualified, bare, _, _, _) in &symbols {
            bare_to_qualified
                .entry(bare.clone())
                .or_insert_with(|| qualified.clone());
        }

        // Dedupe by qualified name (first wins), build entities + a name->index map.
        let mut entities: Vec<KnowledgeEntity> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (qualified, _bare, kind, _body, _chunk_idx) in &symbols {
            if index.contains_key(qualified) {
                continue;
            }
            index.insert(qualified.clone(), entities.len());
            entities.push(KnowledgeEntity {
                id: entity_id(&graph_id, qualified),
                graph_id: Some(graph_id.clone()),
                kind: kind.clone(),
                name: qualified.clone(),
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
                ontology_class_refs: Vec::new(),
                provenance: source.provenance.clone(),
                created_at: now,
                updated_at: None,
                valid_from: Some(now),
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
            // AST-level calls: each (caller, callee) is a real call expression,
            // emitted by treesitter as bare names. Resolve the caller (and a
            // local callee) against the document's bare→qualified map; a
            // non-local callee stays a bare name-only ref for cross-file
            // resolution in `extract_into`.
            for (caller, callee) in calls {
                if caller == callee {
                    continue;
                }
                let Some(caller_qual) = bare_to_qualified.get(caller) else {
                    continue;
                };
                let Some(&subject_index) = index.get(caller_qual) else {
                    continue;
                };
                let object_qual = bare_to_qualified.get(callee).cloned();
                let object_key = object_qual.clone().unwrap_or_else(|| callee.clone());
                if !seen.insert((caller_qual.clone(), object_key.clone())) {
                    continue;
                }
                let object_ref = if let Some(oq) = &object_qual {
                    entity_ref(&entities[index[oq]])
                } else {
                    EntityRef {
                        id: None,
                        kind: None,
                        name: Some(callee.clone()),
                        aliases: Vec::new(),
                    }
                };
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, caller_qual, &object_key),
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
            // Co-occurrence fallback: a symbol's bare name appears in another
            // symbol's body. Match on bare names (the body holds bare tokens);
            // form the edge between the corresponding qualified entities.
            for (subject_qual, _subject_bare, _kind, body, _chunk_idx) in &symbols {
                let Some(&subject_index) = index.get(subject_qual) else {
                    continue;
                };
                for (object_qual, object_bare, _, _, _) in &symbols {
                    if object_qual == subject_qual || !mentions(body, object_bare) {
                        continue;
                    }
                    if !seen.insert((subject_qual.clone(), object_qual.clone())) {
                        continue;
                    }
                    let object_index = index[object_qual];
                    relationships.push(KnowledgeRelationship {
                        id: relationship_id(&graph_id, subject_qual, object_qual),
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
        for (qualified, _bare, _kind, _body, chunk_idx) in &symbols {
            if let Some(&entity_idx) = index.get(qualified) {
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
                ontology_class_refs: Vec::new(),
                provenance: source.provenance.clone(),
                created_at: now,
                updated_at: None,
                valid_from: Some(now),
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
        // against the caller-maintained global name→id index. Each entity is
        // registered under both its qualified name and its bare tail so AST
        // callees (bare) resolve (RFC-0020 T2).
        if let Some(index) = name_index {
            for entity in &extracted.entities {
                register_in_name_index(index, entity);
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

/// Composes a code entity's qualified identity `{repo}/{path}::{bare}` (RFC-0020).
/// `{repo}` is the stable-source-key; when absent the path alone qualifies; when
/// both are absent the bare name is returned (degraded but stable). The git
/// remote + revision are metadata properties (Phase 4), never identity.
fn qualified_symbol_name(repo: Option<&str>, path: Option<&str>, bare: &str) -> String {
    let mut prefix = String::new();
    if let Some(r) = repo.filter(|r| !r.is_empty()) {
        prefix.push_str(r);
    }
    if let Some(p) = path.filter(|p| !p.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(p);
    }
    if prefix.is_empty() {
        bare.to_owned()
    } else {
        format!("{prefix}::{bare}")
    }
}

/// Registers an entity in the cross-file name index under BOTH its qualified
/// name (primary) and its bare tail (secondary), so AST callees — which
/// treesitter emits as bare names — resolve against qualified entities
/// (RFC-0020 T2). Bare collisions are last-write-wins (a documented Phase-1
/// degradation: a colliding bare callee may resolve to the wrong target;
/// removed by a Phase 2 scope-wide symbol table).
pub(crate) fn register_in_name_index(
    index: &mut HashMap<String, String>,
    entity: &KnowledgeEntity,
) {
    index.insert(entity.name.clone(), entity.id.to_string());
    if let Some(bare) = entity.name.rsplit("::").next() {
        if bare != entity.name {
            index.insert(bare.to_owned(), entity.id.to_string());
        }
    }
}

/// Reject entities that aren't real concepts — punctuation tokens, single-char
/// symbols, code-block delimiters, common type annotations, YAML keys.
/// Returns true = "this is noise, skip it."
///
/// Reference implementation for the TS `extract-knowledge` noise filter
/// (RFC-0020 T5): no longer called from the Rust extractor after T3 removed
/// document→Concept emission, but kept as the canonical logic the TS op ports
/// (plus a doc-heading-generic blocklist).
#[allow(dead_code)]
fn is_noise_concept(name: &str) -> bool {
    if name.len() < 3 {
        return true;
    }
    // Must contain at least one alphanumeric char (reject punctuation-only).
    if !name.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    let lower = name.to_lowercase();
    // Common type annotations / system words that aren't real concepts.
    const TYPE_NOISE: &[&str] = &[
        "str",
        "string",
        "int",
        "float",
        "bool",
        "void",
        "null",
        "none",
        "nil",
        "true",
        "false",
        "self",
        "super",
        "this",
        "type",
        "kind",
        "value",
        "name",
        "pub",
        "var",
        "let",
        "const",
        "fn",
        "def",
        "class",
        "struct",
        "enum",
        "import",
        "export",
        "return",
        "async",
        "await",
        "yield",
        "static",
        "u8",
        "u16",
        "u32",
        "u64",
        "i8",
        "i16",
        "i32",
        "i64",
        "f32",
        "f64",
        "usize",
        "isize",
        "vec",
        "option",
        "result",
        "box",
        "rc",
        "arc",
        "string",
        "object",
        "array",
        "map",
        "set",
        "list",
        "dict",
        "tuple",
        "models",
        "description",
        "available",
        "contents",
        "approach",
        "append",
        "clone",
        "print",
        "join",
        "exists",
        "encode",
    ];
    if TYPE_NOISE.contains(&lower.as_str()) {
        return true;
    }
    // Reject "key: value" patterns (YAML/TOML keys like "type: string").
    if name.contains(':') && name.split(':').count() == 2 {
        return true;
    }
    // Reject if it starts with a non-alpha char (likely code noise).
    if !name.starts_with(|c: char| c.is_alphabetic()) {
        return true;
    }
    false
}

/// Rejects code-symbol names that are too generic to be useful graph nodes —
/// language primitives and ubiquitous one-word methods (`new`, `clone`, `len`,
/// `fmt`, …) that, as bare names, collide across every crate and become massive
/// cross-cutting hubs with no stable identity (RFC-0020 Phase 1).
///
/// Tuned for CODE, so unlike [`is_noise_concept`] it does NOT reject short
/// names — `tx`, `db`, `id`, `kv` are meaningful identifiers in code. Only the
/// bare-generic set is blocked. This is the pre-qualified-identity filter: once
/// `parse_symbol` emits qualified identities (`{repo}/{path}::{module}::{name}`,
/// RFC-0020 Phase 1), the generic-method portion of this list can be relaxed and
/// only the true type primitives (`str`, `vec`, `option`, …) kept.
///
/// Returns true = "this symbol is noise, skip it."
fn is_noise_symbol(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }
    // Punctuation-only / non-alphanumeric / non-alpha-leading sanity.
    if !lower.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    if !lower.starts_with(|c: char| c.is_alphabetic()) {
        return true;
    }
    // Bare-generic names. Primitives/type words first, then the ubiquitous
    // one-word methods named in RFC-0020 Phase 1 and the AgentZero indexing
    // guidance (`get`, `str`, `append`, `new`, `clone`, `read`, `write`, …).
    const SYMBOL_NOISE: &[&str] = &[
        // Language primitives & type words.
        "str", "string", "int", "integer", "float", "double", "bool", "boolean", "void", "null",
        "none", "nil", "true", "false", "self", "super", "this", "type", "kind", "value", "pub",
        "var", "let", "const", "static", "object", "array", "map", "set", "list", "dict", "tuple",
        "vector", "vec", "option", "result", "box", "rc", "arc", "ref", "u8", "u16", "u32", "u64",
        "i8", "i16", "i32", "i64", "f32", "f64", "usize", "isize",
        // Generic ubiquitous one-word symbols — bare, they collide across every
        // crate and dominate centrality without a stable identity.
        "new", "clone", "copy", "len", "fmt", "format", "print", "log", "get", "set", "run", "init",
        "send", "recv", "read", "write", "open", "close", "append", "name", "main",
    ];
    SYMBOL_NOISE.contains(&lower.as_str())
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
    use super::{is_noise_symbol, mentions, qualified_symbol_name};

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

    #[test]
    fn is_noise_symbol_blocks_bare_generics_and_primitives() {
        // RFC-0020 Phase 1 + AgentZero guidance bare-generic set.
        for n in [
            "new", "clone", "len", "fmt", "log", "get", "set", "run", "read", "write", "append",
            "send", "name", "main", "init",
        ] {
            assert!(is_noise_symbol(n), "{n:?} should be noise");
        }
        // Language primitives / type words.
        for n in [
            "str", "Vec", "Option", "Result", "bool", "void", "None", "u32", "usize", "Self",
            "self", "type", "value",
        ] {
            assert!(is_noise_symbol(n), "{n:?} should be noise");
        }
    }

    #[test]
    fn is_noise_symbol_keeps_meaningful_symbols() {
        // Real, qualified-looking, or specific symbols survive.
        for n in [
            "respond",
            "persist_turn",
            "parse_symbol",
            "GraphExtractor",
            "NativeProvider",
            "sqlite_bootstrap",
            "handle_create",
            "recall",
        ] {
            assert!(!is_noise_symbol(n), "{n:?} should NOT be noise");
        }
        // Short code identifiers are meaningful — must NOT be filtered by length.
        for n in ["tx", "db", "id", "kv", "rs"] {
            assert!(
                !is_noise_symbol(n),
                "{n:?} should NOT be noise (short but meaningful)"
            );
        }
    }

    #[test]
    fn is_noise_symbol_rejects_garbage() {
        for n in ["", "   ", "{}", "123", "_", "..."] {
            assert!(is_noise_symbol(n), "{n:?} should be noise");
        }
    }

    #[test]
    fn qualified_symbol_name_formats_identity() {
        // Full {repo}/{path}::{bare}.
        assert_eq!(
            qualified_symbol_name(Some("my-repo"), Some("src/lib.rs"), "alpha"),
            "my-repo/src/lib.rs::alpha"
        );
        // No repo → path-qualified.
        assert_eq!(
            qualified_symbol_name(None, Some("lib.rs"), "alpha"),
            "lib.rs::alpha"
        );
        // No path → repo-qualified.
        assert_eq!(
            qualified_symbol_name(Some("my-repo"), None, "alpha"),
            "my-repo::alpha"
        );
        // Neither → bare (degraded but stable).
        assert_eq!(qualified_symbol_name(None, None, "alpha"), "alpha");
        // Empty repo/path are treated as absent.
        assert_eq!(
            qualified_symbol_name(Some(""), Some("lib.rs"), "alpha"),
            "lib.rs::alpha"
        );
    }
}
