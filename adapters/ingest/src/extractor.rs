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
use engram_code::{SymbolCandidate, SymbolIndex};
use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use serde_json::Value as JsonValue;

use crate::{
    hash::content_hash,
    source_key::{
        BRANCH_KEY, DOCUMENT_ID_KEY, REPOSITORY_KEY, REVISION_KEY, SOURCE_PATH_KEY,
        STABLE_SOURCE_KEY,
    },
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
    /// Unresolved-reference ledger (RFC-0020 Phase 2): every cross-file
    /// reference that did not settle, with its candidates. Persistence +
    /// the orphan sweep land in T6; extraction carries it.
    pub unresolved: Vec<engram_domain::UnresolvedReference>,
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
        self.extract_with_calls(source, document, chunks, None, None, None)
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
        structural: Option<&engram_code::StructuralEdges>,
        frameworks: Option<&engram_code::FrameworkFacts>,
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

        // RFC-0020 rev: the entity NAME is the bare logical symbol (function/
        // class/etc). The disambiguating repo/path/branch live in `source_refs`
        // + provenance, NOT jammed into the name. `is_noise_symbol` still drops
        // bare generics. `qualified` here == `bare` (kept as a pair so the
        // co-occurrence / AST-callee matching + `register_in_name_index` are
        // unchanged); the entity `id` stays unique via `graph_id + name`.
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
                let Some((kind, name)) = parse_symbol(anchor) else {
                    continue;
                };
                // RFC-0020 Phase 2: `name` is the receiver-qualified logical
                // name (`Foo::bar`) when the declaration is nested, bare at
                // top level. Noise filtering applies to the bare tail.
                let bare = engram_code::bare_tail(&name).unwrap_or(&name).to_owned();
                if bare.is_empty() || engram_code::is_noise_symbol(&bare) {
                    continue;
                }
                symbols.push((name, bare, kind, chunk.text.clone(), chunk_idx));
            }
        } else {
            // RFC-0020 T3: non-code documents emit NO graph entities — the naive
            // heading-as-node rule is gone. Documents are chunks-only at ingest;
            // the LLM `extract-knowledge` op produces the concept sub-graph. The
            // graph record above is created unconditionally so `listGraphs` can
            // still discover documents (the extract-knowledge op relies on this).
        }

        // Document symbol table (RFC-0020 Phase 2): bare tail → ALL local
        // qualified candidates. A unique candidate qualifies the reference;
        // an ambiguous one stays bare for cross-file resolution (never an
        // arbitrary pick — the Phase-1 first-wins map is gone).
        let mut bare_to_qualified: HashMap<String, Vec<String>> = HashMap::new();
        for (qualified, bare, _, _, _) in &symbols {
            bare_to_qualified
                .entry(bare.clone())
                .or_default()
                .push(qualified.clone());
        }

        // RFC-0020 rev: git provenance (repository/branch/revision) is metadata,
        // not identity. Lift the clean keys off the source's metadata (stamped by
        // the scanner from the detect_git tuple) so each code entity carries them
        // in its `record_json` — reachable by the cc entity-detail route. Identity
        // (the bare name) is unchanged; re-indexing from a different branch only
        // updates these metadata values.
        let entity_git_meta: Option<Metadata> = source.metadata.as_ref().and_then(|m| {
            let mut g = Metadata::default();
            for key in [REPOSITORY_KEY, BRANCH_KEY, REVISION_KEY] {
                if let Some(v) = m.get(key) {
                    g.insert(key.to_owned(), v.clone());
                }
            }
            if g.is_empty() { None } else { Some(g) }
        });

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
                metadata: entity_git_meta.clone(),

                archived_at: None,
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
                let Some(caller_qual) = local_qualify(&bare_to_qualified, caller) else {
                    continue;
                };
                let Some(&subject_index) = index.get(caller_qual.as_str()) else {
                    continue;
                };
                let object_qual = local_qualify(&bare_to_qualified, callee);
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
                    id: relationship_id(&graph_id, &caller_qual, &object_key),
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

                    archived_at: None,
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

                        archived_at: None,
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

                archived_at: None,
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

                archived_at: None,
            });
        }

        // RFC-0020 Phase 2 typed structural edges. Containment is
        // intra-document (both endpoints are local entities); inheritance
        // targets may be external (name-only, resolved cross-file in
        // `extract_into`); imports form a File entity → Module entity edge
        // per import path (graph-only entities, like the Repository entity).
        if let Some(edges) = structural {
            let file_name = document
                .path
                .clone()
                .unwrap_or_else(|| document.id.to_string());
            let file_id = entity_id(&graph_id, &file_name);
            let file_ref = EntityRef {
                id: Some(file_id.clone()),
                kind: Some("file".to_owned()),
                name: Some(file_name.clone()),
                aliases: Vec::new(),
            };
            // Every code document gets its File entity (resolution needs
            // importer and non-importer files alike), not just importers.
            entities.push(KnowledgeEntity {
                id: file_id.clone(),
                graph_id: Some(graph_id.clone()),
                kind: EntityKind::File,
                name: file_name.clone(),
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
                metadata: entity_git_meta.clone(),
                archived_at: None,
            });
            for path in &edges.imports {
                let module_id = entity_id(&graph_id, &format!("module:{path}"));
                let module_ref = EntityRef {
                    id: Some(module_id.clone()),
                    kind: Some("module".to_owned()),
                    name: Some(path.clone()),
                    aliases: Vec::new(),
                };
                entities.push(KnowledgeEntity {
                    id: module_id,
                    graph_id: Some(graph_id.clone()),
                    kind: EntityKind::Module,
                    name: path.clone(),
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
                    metadata: entity_git_meta.clone(),
                    archived_at: None,
                });
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, &file_name, path),
                    graph_id: Some(graph_id.clone()),
                    subject: file_ref.clone(),
                    predicate: "imports".to_owned(),
                    object: module_ref,
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(1.0),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                    archived_at: None,
                });
            }
            let mut structural_edge = |predicate: &str, from: &str, to: &str| {
                let subject = index.get(from).map(|&i| entity_ref(&entities[i]));
                let Some(subject) = subject else { return };
                // Containment targets are same-document by construction —
                // resolve the id directly; inheritance targets may be
                // external and stay name-only for cross-file resolution.
                let object = match predicate {
                    "contains" => index.get(to).map(|&i| entity_ref(&entities[i])),
                    _ => None,
                }
                .unwrap_or(EntityRef {
                    id: None,
                    kind: None,
                    name: Some(to.to_owned()),
                    aliases: Vec::new(),
                });
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, from, &format!("{predicate}:{to}")),
                    graph_id: Some(graph_id.clone()),
                    subject,
                    predicate: predicate.to_owned(),
                    object,
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(1.0),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                    archived_at: None,
                });
            };
            for (parent, member) in &edges.contains {
                structural_edge("contains", parent, member);
            }
            for (child, base) in &edges.extends {
                structural_edge("extends", child, base);
            }
            for (child, iface) in &edges.implements {
                structural_edge("implements", child, iface);
            }
        }

        // RFC-0020 Phase 2 framework patterns: routes become Endpoint
        // entities (`GET /users`) wired to their handlers via `routes_to`;
        // React callbacks become `calls` edges from the component. Handlers
        // resolve against the document symbol table; handlers declared in
        // other files stay name-only for cross-file resolution.
        if let Some(facts) = frameworks {
            for route in &facts.routes {
                let endpoint_name = format!("{} {}", route.method, route.path);
                let endpoint_id = entity_id(&graph_id, &endpoint_name);
                entities.push(KnowledgeEntity {
                    id: endpoint_id.clone(),
                    graph_id: Some(graph_id.clone()),
                    kind: EntityKind::Endpoint,
                    name: endpoint_name.clone(),
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
                    metadata: entity_git_meta.clone(),
                    archived_at: None,
                });
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, &endpoint_name, &route.handler),
                    graph_id: Some(graph_id.clone()),
                    subject: EntityRef {
                        id: Some(endpoint_id),
                        kind: Some("endpoint".to_owned()),
                        name: Some(endpoint_name),
                        aliases: Vec::new(),
                    },
                    predicate: "routes_to".to_owned(),
                    object: EntityRef {
                        id: None,
                        kind: None,
                        name: Some(route.handler.clone()),
                        aliases: Vec::new(),
                    },
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(1.0),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                    archived_at: None,
                });
            }
            for (component, handler) in &facts.callbacks {
                let Some(&component_index) = index.get(component) else {
                    continue;
                };
                relationships.push(KnowledgeRelationship {
                    id: relationship_id(&graph_id, component, &format!("jsx:{handler}")),
                    graph_id: Some(graph_id.clone()),
                    subject: entity_ref(&entities[component_index]),
                    predicate: "calls".to_owned(),
                    object: EntityRef {
                        id: None,
                        kind: None,
                        name: Some(handler.clone()),
                        aliases: Vec::new(),
                    },
                    scope: source.scope.clone(),
                    evidence: Vec::new(),
                    confidence: Some(0.9),
                    provenance: source.provenance.clone(),
                    created_at: now,
                    updated_at: None,
                    archived_at: None,
                });
            }
        }

        Ok(ExtractedGraph {
            graph,
            entities,
            relationships,
            chunk_entities,
            unresolved: Vec::new(),
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
        name_index: Option<&mut SymbolIndex>,
    ) -> CoreResult<ExtractedGraph>
    where
        R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
    {
        let mut extracted = Self.extract(source, document, chunks)?;

        // Cross-file edge resolution (C1): fill name-only calls object refs
        // against the caller-maintained scope-wide symbol table. Each entity
        // is registered under both its qualified name and its bare tail with
        // repo/path discriminators; resolution prefers same-document then
        // same-repo candidates (RFC-0020 Phase 2).
        if let Some(index) = name_index {
            let repo = source
                .metadata
                .as_ref()
                .and_then(|m| m.get(REPOSITORY_KEY))
                .and_then(|v| v.as_str());
            let path = document.path.as_deref();
            register_entities(index, &extracted.entities, repo, path);
            // Resolution runs the same-doc / same-repo / unique / ledger
            // ladder (the scanner path adds cross-file context the same way).
            extracted.unresolved =
                engram_code::resolve_refs(index, &mut extracted.relationships, repo, path);
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
        // Namespaces/modules (C++/C# `namespace`, Rust `mod`) carry the
        // receiver chain for their members.
        "module" | "namespace" | "mod" => EntityKind::Module,
        _ => return None,
    };
    Some((kind, name.to_owned()))
}

/// Unique local qualified candidate for a bare name, if exactly one exists.
/// Ambiguous bare names (two receivers defining the same tail in one file)
/// intentionally return None — the reference stays bare for cross-file
/// resolution instead of an arbitrary pick.
fn local_qualify(bare_to_qualified: &HashMap<String, Vec<String>>, bare: &str) -> Option<String> {
    let candidates = bare_to_qualified.get(bare)?;
    match candidates.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// Registers entities in the scope-wide symbol table under BOTH their
/// qualified name (primary) and bare tail (secondary), so AST callees —
/// which treesitter emits as bare names — resolve against qualified
/// entities. Appends per key: collisions coexist as candidates with their
/// repo/path discriminators (RFC-0020 Phase 2 multi-candidate table — the
/// Phase-1 last-write-wins degradation is gone).
pub fn register_entities(
    index: &mut SymbolIndex,
    entities: &[KnowledgeEntity],
    repo: Option<&str>,
    path: Option<&str>,
) {
    for entity in entities {
        // File/Module/Repository entities are not call targets — registering
        // their path-shaped names would pollute bare-name resolution (an
        // import path like `fmt` colliding with a `fmt` fn; the repository
        // key likewise). This also covers namespace Module entities.
        if matches!(
            entity.kind,
            EntityKind::File | EntityKind::Module | EntityKind::Repository
        ) {
            continue;
        }
        index.register(
            &entity.name,
            SymbolCandidate {
                id: entity.id.to_string(),
                name: entity.name.clone(),
                repo: repo.map(str::to_owned),
                path: path.map(str::to_owned),
            },
        );
    }
}

/// Fills name-only object refs (`calls`, `extends`, `implements`) against the
/// scope-wide symbol table, preferring same-document then same-repo
/// candidates. Ambiguous or unknown references stay name-only (recorded by
/// the Phase-2 ledger, T5).
pub fn resolve_call_refs(
    index: &SymbolIndex,
    relationships: &mut [KnowledgeRelationship],
    repo: Option<&str>,
    path: Option<&str>,
) -> Vec<engram_domain::UnresolvedReference> {
    // Delegates to engram-code's Phase-2 resolver (receiver hints, the
    // same-doc/same-repo/unique/ledger ladder); returns the ledger records
    // for callers that persist them (T6).
    engram_code::resolve_refs(index, relationships, repo, path)
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
    use engram_code::is_noise_symbol;

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
}
