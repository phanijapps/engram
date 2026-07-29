//! MCP tool handlers that route through the [`EngramProvider`].
//!
//! Each handler borrows the shared [`App`] (provider + fused-per-project scope)
//! and returns [`Result<Value, ToolError>`] so failures surface as JSON-RPC
//! errors rather than being embedded in a success string. The domain-record
//! construction mirrors the proven `memory/mcp-server` pattern.

use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, ConsolidationRequest, DeleteMode, EntityKind, EntityRef,
    ForgetRequest, ForgetTargetType, Id, KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship,
    KnowledgeSource, MemoryContent, MemoryKind, MemoryRecord, MemoryStatus, Policy, Provenance,
    Requester, Retention, RetrievalRequest, RetrievalTargetType, Sensitivity, SourceDocument,
    SourceDocumentKind, SourceKind, SourceLocation, Visibility, WriteMemoryRequest,
};
use engram_ingest::{
    Chunker, MarkdownChunker, PlainTextChunker, PlainTextChunkerOptions, content_hash,
};
use engram_integration::BatchIngestRequest;
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;

// --- shared record helpers ----------------------------------------------------

pub(crate) fn system_actor() -> Actor {
    Actor {
        id: Id::from("engram-mcp"),
        kind: ActorKind::Agent,
        display_name: Some("engram-mcp".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: system_actor(),
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    }
}

pub(crate) fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance(method: &str) -> Provenance {
    Provenance {
        source: "engram-mcp".to_owned(),
        actor: system_actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some(method.to_owned()),
    }
}

pub(crate) fn internal(msg: impl std::fmt::Display) -> ToolError {
    ToolError::new(-32603, msg.to_string())
}

fn invalid(msg: impl std::fmt::Display) -> ToolError {
    ToolError::new(-32602, msg.to_string())
}

/// A required, non-empty string arg; `-32602` (invalid params) if absent/empty.
pub(crate) fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("{key} is required")))
}

/// Parse a `kind` string into an [`EntityKind`], rejecting unknown variants
/// (so `put_entity` honors the caller's kind rather than silently defaulting).
fn parse_entity_kind(kind: &str) -> Result<EntityKind, ToolError> {
    serde_json::from_value(serde_json::Value::String(kind.to_owned()))
        .map_err(|_| invalid(format!("unknown entity kind: {kind}")))
}

/// A recall lane filter: `None` fuses every source; `Some(set)` keeps only the
/// listed [`RetrievalTargetType`]s. Maps user-facing lane names onto the
/// underlying target types.
struct LaneFilter(Option<Vec<RetrievalTargetType>>);

impl LaneFilter {
    fn all() -> Self {
        Self(None)
    }

    fn allows(&self, t: &RetrievalTargetType) -> bool {
        match &self.0 {
            None => true,
            Some(set) => set.iter().any(|x| x == t),
        }
    }
}

fn parse_lanes(raw: &Value) -> LaneFilter {
    use RetrievalTargetType::*;
    let Some(arr) = raw.as_array() else {
        return LaneFilter::all();
    };
    let mut set = Vec::new();
    for lane in arr.iter().filter_map(Value::as_str) {
        match lane {
            "memory" => set.push(Memory),
            "knowledge" | "code" => set.extend([Entity, Relationship, Concept]),
            "docs" => set.extend([Chunk, Document]),
            "beliefs" => set.push(Belief),
            _ => {} // unrecognized lane names are ignored (lenient).
        }
    }
    if set.is_empty() {
        LaneFilter::all()
    } else {
        LaneFilter(Some(set))
    }
}

// --- tools --------------------------------------------------------------------

/// `write_memory`: persist an observation/episode to the memory layer.
pub fn write_memory(app: &App, args: &Value) -> Result<Value, ToolError> {
    let content = req_str(args, "content")?;
    let memory = app.provider.require_memory().map_err(internal)?;
    let request = WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: content.to_owned(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: app.scope.clone(),
        requester: requester(),
        provenance: provenance("mcp-write"),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    };
    let response = block_on(memory.write_memory(request)).map_err(internal)?;
    let body = serde_json::to_string_pretty(&response).unwrap_or_else(|_| "OK".to_owned());
    Ok(protocol::text_content(body))
}

/// `forget`: delete, redact, tombstone, or archive a memory by target id.
pub fn forget(app: &App, args: &Value) -> Result<Value, ToolError> {
    let target_id = req_str(args, "target_id")?;
    let mode = match args["mode"].as_str().unwrap_or("tombstone") {
        "delete" => DeleteMode::Delete,
        "redact" => DeleteMode::Redact,
        "tombstone" => DeleteMode::Tombstone,
        "archive" => DeleteMode::Archive,
        other => {
            return Err(invalid(format!(
                "unknown mode: {other}; expected delete|redact|tombstone|archive"
            )));
        }
    };
    let memory = app.provider.require_memory().map_err(internal)?;
    let request = ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: target_id.to_owned(),
        scope: app.scope.clone(),
        requester: requester(),
        mode,
        reason: None,
    };
    let response = block_on(memory.forget(request)).map_err(internal)?;
    let body = serde_json::to_string_pretty(&response).unwrap_or_else(|_| "OK".to_owned());
    Ok(protocol::text_content(body))
}

/// `put_entity`: add an entity to the knowledge graph (upsert by `name`;
/// honors the `kind` arg, rejecting unknown kinds).
pub fn put_entity(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = req_str(args, "name")?;
    let kind = parse_entity_kind(args["kind"].as_str().unwrap_or("concept"))?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let entity = KnowledgeEntity {
        id: Id::from(name),
        graph_id: None,
        kind,
        name: name.to_owned(),
        aliases: Vec::new(),
        scope: app.scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance("mcp-put-entity"),
        created_at: Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    };
    let stored = block_on(knowledge.put_entity(entity)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Entity '{}' ({:?}) stored.",
        stored.name, stored.kind
    )))
}

/// `put_relationship`: add a (subject, predicate, object) edge to the graph.
/// The ID uses a unit-separator-delimited tuple so distinct (s, p, o) triples
/// cannot collide regardless of the strings involved.
pub fn put_relationship(app: &App, args: &Value) -> Result<Value, ToolError> {
    let subject = req_str(args, "subject")?;
    let predicate = req_str(args, "predicate")?;
    let object = req_str(args, "object")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let rel = KnowledgeRelationship {
        id: Id::from(format!("{subject}\u{1f}{predicate}\u{1f}{object}")),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from(subject)),
            kind: None,
            name: Some(subject.to_owned()),
            aliases: Vec::new(),
        },
        predicate: predicate.to_owned(),
        object: EntityRef {
            id: Some(Id::from(object)),
            kind: None,
            name: Some(object.to_owned()),
            aliases: Vec::new(),
        },
        scope: app.scope.clone(),
        evidence: Vec::new(),
        confidence: None,
        provenance: provenance("mcp-put-relationship"),
        created_at: Utc::now(),
        updated_at: None,
    };
    block_on(knowledge.put_relationship(rel)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "{subject} -[{predicate}]-> {object} stored."
    )))
}

/// `recall`: fused retrieval across memory + knowledge + beliefs for the
/// project scope. The optional `lanes` array restricts the result to a subset
/// of target types (memory / knowledge / docs / beliefs); absent or empty
/// `lanes` fuses everything. `limit` defaults to 10 (clamped 1..=100).
pub fn recall(app: &App, args: &Value) -> Result<Value, ToolError> {
    let query = req_str(args, "query")?;
    let lanes = parse_lanes(&args["lanes"]);
    let limit = Some(
        args["limit"]
            .as_u64()
            .map(|n| n as u32)
            .unwrap_or(10)
            .clamp(1, 100),
    );
    let recall = app.provider.require_recall().map_err(internal)?;
    let request = RetrievalRequest {
        query: query.to_owned(),
        scope: app.scope.clone(),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit,
        budget: None,
        include_explanations: Some(true),
    };
    let payload = block_on(recall.recall(request)).map_err(internal)?;
    let items: Vec<&str> = payload
        .items
        .iter()
        .filter(|i| lanes.allows(&i.target_type))
        .map(|i| i.content.as_str())
        .collect();
    let body = if items.is_empty() {
        "No results.".to_owned()
    } else {
        items.join("\n---\n")
    };
    Ok(protocol::text_content(body))
}

/// `consolidate`: run reflection + decay over the project scope.
pub fn consolidate(app: &App, args: &Value) -> Result<Value, ToolError> {
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let consolidation = app.provider.require_consolidation().map_err(internal)?;
    let request = ConsolidationRequest {
        scope: app.scope.clone(),
        requester: requester(),
        since: None,
        until: None,
        strategy: None,
        dry_run: Some(dry_run),
    };
    let run = block_on(consolidation.consolidate(request)).map_err(internal)?;
    let summary: Vec<String> = run
        .tasks
        .iter()
        .map(|t| {
            format!(
                "{:?}: {:?} (read={}, written={})",
                t.task,
                t.status,
                t.items_read.unwrap_or(0),
                t.items_written.unwrap_or(0)
            )
        })
        .collect();
    Ok(protocol::text_content(format!(
        "Consolidation {:?}: {} task(s).\n{}",
        run.status,
        run.tasks.len(),
        summary.join("\n")
    )))
}

/// `store_knowledge`: bulk distill-write. Takes facts + entities + relationships
/// (extracted by the agent skill) and writes them through one `BatchIngest`
/// call. The guarantee is **best-effort, not ACID** — the batch's per-step
/// status is surfaced so a partial failure is visible, never hidden.
///
/// Entries missing a required field are skipped (lenient): a malformed fact /
/// entity / relationship does not abort the batch; valid entries still land.
pub fn store_knowledge(app: &App, args: &Value) -> Result<Value, ToolError> {
    let batch = app.provider.require_batch().map_err(internal)?;
    let idempotency_key = args["idempotency_key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "store-knowledge-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
        });

    let scope = app.scope.clone();
    let prov = provenance("mcp-store-knowledge");
    let pol = policy();

    // Facts (memories). Entries without non-empty `content` are skipped.
    let facts: Vec<MemoryRecord> = args["facts"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, f)| {
            let content = f["content"].as_str().filter(|s| !s.is_empty())?;
            Some(MemoryRecord {
                id: Id::from(format!("{idempotency_key}#{i}")),
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: content.to_owned(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                scope: scope.clone(),
                provenance: prov.clone(),
                policy: pol.clone(),
                status: MemoryStatus::Active,
                links: Vec::new(),
                assertions: Vec::new(),
                created_at: Utc::now(),
                updated_at: None,
                metadata: None,
            })
        })
        .collect();

    // Entities. Entries without a non-empty `name` or with an unknown kind are skipped.
    let entities: Vec<KnowledgeEntity> = args["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let name = e["name"].as_str().filter(|s| !s.is_empty())?;
            let kind = parse_entity_kind(e["kind"].as_str().unwrap_or("concept")).ok()?;
            Some(KnowledgeEntity {
                id: Id::from(name),
                graph_id: None,
                kind,
                name: name.to_owned(),
                aliases: Vec::new(),
                scope: scope.clone(),
                source_refs: Vec::new(),
                concept_refs: Vec::new(),
                ontology_class_refs: Vec::new(),
                provenance: prov.clone(),
                created_at: Utc::now(),
                updated_at: None,
                valid_from: None,
                valid_until: None,
                metadata: None,
            })
        })
        .collect();

    // Relationships. Entries missing subject/predicate/object are skipped.
    let relationships: Vec<KnowledgeRelationship> = args["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let subject = r["subject"].as_str().filter(|s| !s.is_empty())?;
            let predicate = r["predicate"].as_str().filter(|s| !s.is_empty())?;
            let object = r["object"].as_str().filter(|s| !s.is_empty())?;
            Some(KnowledgeRelationship {
                id: Id::from(format!("{subject}\u{1f}{predicate}\u{1f}{object}")),
                graph_id: None,
                subject: EntityRef {
                    id: Some(Id::from(subject)),
                    kind: None,
                    name: Some(subject.to_owned()),
                    aliases: Vec::new(),
                },
                predicate: predicate.to_owned(),
                object: EntityRef {
                    id: Some(Id::from(object)),
                    kind: None,
                    name: Some(object.to_owned()),
                    aliases: Vec::new(),
                },
                scope: scope.clone(),
                evidence: Vec::new(),
                confidence: None,
                provenance: prov.clone(),
                created_at: Utc::now(),
                updated_at: None,
            })
        })
        .collect();

    let fact_count = facts.len();
    let entity_count = entities.len();
    let rel_count = relationships.len();
    // How many entries were dropped at the edge (missing required fields).
    let skipped = args["facts"]
        .as_array()
        .map_or(0, Vec::len)
        .saturating_sub(fact_count)
        + args["entities"]
            .as_array()
            .map_or(0, Vec::len)
            .saturating_sub(entity_count)
        + args["relationships"]
            .as_array()
            .map_or(0, Vec::len)
            .saturating_sub(rel_count);
    let skipped_note = if skipped > 0 {
        format!(", {skipped} malformed entries skipped")
    } else {
        String::new()
    };

    let request = BatchIngestRequest {
        idempotency_key,
        scope,
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts,
        entities,
        relationships,
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    let outcome = block_on(batch.ingest(request)).map_err(internal)?;

    let steps: Vec<String> = outcome
        .steps
        .iter()
        .map(|s| format!("{:?}: {:?}", s.step, s.status))
        .collect();
    Ok(protocol::text_content(format!(
        "Batch {:?} (guarantee: {:?}). {} fact(s), {} entities, {} relationship(s){skipped_note}. [{}]",
        outcome.status,
        outcome.guarantee,
        fact_count,
        entity_count,
        rel_count,
        steps.join(", ")
    )))
}

/// `index_docs`: chunk a Markdown (or text) document into retrieval-friendly
/// sections and persist them through `BatchIngest` so the doc is retrievable via
/// `recall` (the `docs` lane). The Markdown structure (headers / code / prose)
/// is preserved as chunk kinds + line-span provenance.
pub fn index_docs(app: &App, args: &Value) -> Result<Value, ToolError> {
    let text = req_str(args, "content")?;
    let path = args["path"].as_str().map(str::to_owned);
    let kind = match args["kind"].as_str().unwrap_or("markdown") {
        "markdown" => SourceDocumentKind::Markdown,
        "text" => SourceDocumentKind::Text,
        other => {
            return Err(invalid(format!(
                "unknown doc kind: {other}; expected markdown|text"
            )));
        }
    };
    let mime = if kind == SourceDocumentKind::Text {
        "text/plain"
    } else {
        "text/markdown"
    };

    let candidates = match kind {
        SourceDocumentKind::Text => PlainTextChunker::new(PlainTextChunkerOptions::default())
            .map_err(internal)?
            .chunk(text)
            .map_err(invalid)?,
        _ => MarkdownChunker::new()
            .map_err(internal)?
            .chunk(text)
            .map_err(invalid)?,
    };
    if candidates.is_empty() {
        return Err(invalid("document produced no chunks"));
    }

    let now = Utc::now();
    let scope = app.scope.clone();
    let prov = provenance("mcp-index-docs");
    let pol = policy();
    let doc_key = format!("doc-{}", content_hash(text));
    let source_id = Id::from(format!("source-{doc_key}"));
    let doc_id = Id::from(doc_key.clone());

    let source = KnowledgeSource {
        id: source_id.clone(),
        kind: SourceKind::Upload,
        scope: scope.clone(),
        name: path.clone().unwrap_or_else(|| "indexed-doc".to_owned()),
        uri: None,
        version: None,
        policy: pol.clone(),
        provenance: prov.clone(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };
    let document = SourceDocument {
        id: doc_id.clone(),
        source_id: source_id.clone(),
        kind,
        uri: None,
        path: path.clone(),
        title: None,
        mime_type: Some(mime.to_owned()),
        language: None,
        version: None,
        content_hash: content_hash(text),
        provenance: prov.clone(),
        policy: pol.clone(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };

    let chunks: Vec<KnowledgeChunk> = candidates
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let chunk_hash = content_hash(&c.text);
            KnowledgeChunk {
                id: Id::from(format!("{doc_id}#{i}")),
                document_id: doc_id.clone(),
                source_id: source_id.clone(),
                kind: c.kind,
                text: c.text,
                summary: None,
                location: c.location.map(|loc| SourceLocation {
                    path: path.clone(),
                    ..loc
                }),
                entities: Vec::new(),
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: chunk_hash,
                provenance: prov.clone(),
                policy: pol.clone(),
                created_at: now,
                updated_at: None,
                metadata: None,
            }
        })
        .collect();
    let chunk_count = chunks.len();

    let request = BatchIngestRequest {
        idempotency_key: format!("index-docs-{doc_key}"),
        scope,
        source: Some(source),
        documents: vec![document],
        chunks,
        facts: Vec::new(),
        entities: Vec::new(),
        relationships: Vec::new(),
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    let batch = app.provider.require_batch().map_err(internal)?;
    let outcome = block_on(batch.ingest(request)).map_err(internal)?;
    Ok(protocol::text_content(format!(
        "Indexed {} chunk(s). Batch {:?} (guarantee: {:?}).",
        chunk_count, outcome.status, outcome.guarantee
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{OntologyConfig, TaxonomyConfig};
    use crate::scope::project_scope;
    use engram_domain::ScopeMappingStrategy;
    use engram_integration::{
        CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
    };
    use serde_json::json;

    fn test_app(dir: &std::path::Path) -> App {
        let config = EngramConfig::new(
            dir.join("engram_data.db"),
            dir.to_path_buf(),
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "none".to_owned(),
                model: "none".to_owned(),
                dimensions: 384,
                prompt_profile: "query".to_owned(),
                normalization: None,
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        let provider = EngramProvider::open(&config).expect("open provider");
        App {
            provider,
            scope: project_scope("test-project", "default"),
            ontology: OntologyConfig::default(),
            taxonomy: TaxonomyConfig::default(),
        }
    }

    #[test]
    fn knowledge_query_lists_written_entities() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        put_entity(&app, &json!({ "name": "ListMe", "kind": "concept" })).unwrap();
        let q = app
            .provider
            .require_knowledge_query()
            .expect("knowledge_query handle");
        let entities = block_on(q.list_entities(&app.scope)).expect("list_entities");
        assert!(
            entities.iter().any(|e| e.name == "ListMe"),
            "list_entities must include the written entity: {entities:?}"
        );
    }

    #[test]
    fn lexical_feed_upserts() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let feed = app
            .provider
            .require_lexical_feed()
            .expect("lexical_feed handle");
        block_on(feed.upsert("Zorblax", "Zorblax function")).expect("upsert");
        block_on(feed.upsert_batch(&[("a".into(), "alpha".into()), ("b".into(), "beta".into())]))
            .expect("upsert_batch");
    }

    #[test]
    fn write_memory_rejects_missing_content() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let err = write_memory(&app, &json!({})).unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn put_entity_honors_kind_and_rejects_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        assert!(put_entity(&app, &json!({ "name": "X", "kind": "concept" })).is_ok());
        assert!(put_entity(&app, &json!({ "name": "Y", "kind": "api" })).is_ok());
        // "Service" is not a valid EntityKind → invalid params.
        let err = put_entity(&app, &json!({ "name": "Z", "kind": "Service" })).unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn put_relationship_rejects_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let err = put_relationship(
            &app,
            &json!({ "subject": "a", "predicate": "b" }), // no object
        )
        .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn forget_rejects_unknown_mode() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let err = forget(&app, &json!({ "target_id": "x", "mode": "permanent" })).unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn write_then_recall_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        write_memory(&app, &json!({ "content": "Alice works at Acme" })).unwrap();
        let res = recall(&app, &json!({ "query": "Alice" })).unwrap();
        let body = res["content"][0]["text"].as_str().unwrap();
        assert!(
            body.contains("Alice") || body.contains("Acme"),
            "recall should find the written memory: {body}"
        );
    }

    /// AC #4 — `lanes` restricts the fused result: a memory is visible with no
    /// lanes (or `lanes:["memory"]`) and excluded when only `knowledge` is requested.
    #[test]
    fn recall_lanes_filter_excludes_other_lanes() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        write_memory(&app, &json!({ "content": "lanes-filter-marker" })).unwrap();

        let all = recall(&app, &json!({ "query": "lanes-filter-marker" })).unwrap();
        assert!(
            all["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("lanes-filter-marker")
        );

        let knowledge_only = recall(
            &app,
            &json!({ "query": "lanes-filter-marker", "lanes": ["knowledge"] }),
        )
        .unwrap();
        assert!(
            !knowledge_only["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("lanes-filter-marker"),
            "knowledge lane must exclude memory items"
        );
    }

    /// AC #5 — fused-per-project isolation: a memory written under a different
    /// project workspace is invisible to recall under this project's scope.
    #[test]
    fn workspace_scope_isolates_recall() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let memory = app.provider.require_memory().expect("memory handle");
        let write_req = WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "isolated-secret-token".to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: project_scope("other-project", "default"),
            requester: requester(),
            provenance: provenance("test"),
            policy: policy(),
            links: Vec::new(),
            idempotency_key: None,
        };
        block_on(memory.write_memory(write_req)).expect("write");

        let recall_handle = app.provider.require_recall().expect("recall handle");
        let req = RetrievalRequest {
            query: "isolated-secret-token".to_owned(),
            scope: app.scope.clone(),
            requester: requester(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(10),
            budget: None,
            include_explanations: Some(true),
        };
        let payload = block_on(recall_handle.recall(req)).expect("recall");
        let leaked = payload
            .items
            .iter()
            .any(|i| i.content.contains("isolated-secret-token"));
        assert!(
            !leaked,
            "recall under a different workspace must be isolated"
        );
    }

    /// AC #8 — store_knowledge maps onto BatchIngest and surfaces the
    /// BestEffort guarantee + per-step status for valid input.
    #[test]
    fn store_knowledge_completes_and_surfaces_status() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let res = store_knowledge(
            &app,
            &json!({
                "facts": [{ "content": "Acme builds widgets" }],
                "entities": [
                    { "name": "Acme", "kind": "organization" },
                    { "name": "Widget", "kind": "concept" }
                ],
                "relationships": [{ "subject": "Acme", "predicate": "builds", "object": "Widget" }]
            }),
        )
        .expect("store_knowledge should succeed");
        let body = res["content"][0]["text"].as_str().unwrap();
        assert!(
            body.contains("BestEffort"),
            "must surface the guarantee: {body}"
        );
        assert!(
            body.contains("Complete"),
            "expected Complete for valid input: {body}"
        );

        // Persistence readback: the written fact must be recoverable via recall.
        let rec = recall(&app, &json!({ "query": "Acme" })).unwrap();
        let rec_body = rec["content"][0]["text"].as_str().unwrap();
        assert!(
            rec_body.contains("Acme builds widgets") || rec_body.contains("Acme"),
            "written fact should be recoverable: {rec_body}"
        );
    }

    /// AC #8 — a failed step surfaces `Partial` (not `Complete`) with the
    /// `BestEffort` guarantee, and other steps still land (no rollback). Uses
    /// the batch handle directly with an empty-text fact (the trigger at
    /// `adapters/integration/tests/batch_ingest.rs:342`); `store_knowledge`
    /// surfaces this outcome verbatim.
    #[test]
    fn batch_surfaces_partial_on_a_failed_step() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let batch = app.provider.require_batch().expect("batch handle");
        let req = BatchIngestRequest {
            idempotency_key: "partial-batch".to_owned(),
            scope: app.scope.clone(),
            source: None,
            documents: Vec::new(),
            chunks: Vec::new(),
            facts: vec![MemoryRecord {
                id: Id::from("bad-fact"),
                kind: MemoryKind::Observation,
                content: MemoryContent {
                    text: String::new(),
                    summary: None,
                    entities: Vec::new(),
                    language: None,
                    format: None,
                    structured: None,
                    hash: None,
                },
                scope: app.scope.clone(),
                provenance: provenance("test"),
                policy: policy(),
                status: MemoryStatus::Active,
                links: Vec::new(),
                assertions: Vec::new(),
                created_at: Utc::now(),
                updated_at: None,
                metadata: None,
            }],
            entities: vec![KnowledgeEntity {
                id: Id::from("good-entity"),
                graph_id: None,
                kind: EntityKind::Concept,
                name: "Good".to_owned(),
                aliases: Vec::new(),
                scope: app.scope.clone(),
                source_refs: Vec::new(),
                concept_refs: Vec::new(),
                ontology_class_refs: Vec::new(),
                provenance: provenance("test"),
                created_at: Utc::now(),
                updated_at: None,
                valid_from: None,
                valid_until: None,
                metadata: None,
            }],
            relationships: Vec::new(),
            evidence: Vec::new(),
            embeddings: Vec::new(),
        };
        let outcome = block_on(batch.ingest(req)).expect("ingest");
        let guarantee = format!("{:?}", outcome.guarantee);
        let status = format!("{:?}", outcome.status);
        let steps = format!("{:?}", outcome.steps);
        assert!(guarantee.contains("BestEffort"), "guarantee: {guarantee}");
        assert!(
            status.contains("Partial"),
            "a bad fact should make the batch Partial: {status}"
        );
        assert!(
            steps.contains("Succeeded"),
            "non-failed steps stay landed (no rollback): {steps}"
        );
    }

    /// T10 — index_docs chunks a doc and persists it retrievably.
    #[test]
    fn index_docs_chunks_and_is_retrievable() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let md = "# Design\nThe flubbaz-widget is the core unit.\n";
        let res = index_docs(&app, &json!({ "content": md, "path": "design.md" })).unwrap();
        let body = res["content"][0]["text"].as_str().unwrap();
        assert!(
            body.contains("Indexed") && body.contains("BestEffort"),
            "{body}"
        );

        // The doc chunk is retrievable via recall.
        let rec = recall(&app, &json!({ "query": "flubbaz-widget" })).unwrap();
        let rec_body = rec["content"][0]["text"].as_str().unwrap();
        assert!(
            rec_body.contains("flubbaz-widget"),
            "doc chunk should be retrievable: {rec_body}"
        );
    }

    #[test]
    fn scan_repo_indexes_code_into_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo_dir.path().join("src")).unwrap();
        std::fs::write(
            repo_dir.path().join("src/main.rs"),
            "fn alpha() {}\nfn beta() {}\nfn main() { alpha(); beta(); }\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        let res = crate::codegraph::scan_repo(
            &app,
            &json!({ "path": repo_dir.path().to_str().unwrap() }),
        )
        .unwrap();
        let _body = res["content"][0]["text"].as_str().unwrap();
        // The scanned functions land as entities in the project scope.
        let q = app
            .provider
            .require_knowledge_query()
            .expect("knowledge_query handle");
        let entities = block_on(q.list_entities(&app.scope)).unwrap();
        assert!(
            entities
                .iter()
                .any(|e| e.name == "alpha" || e.name == "beta"),
            "scan_repo must index the functions: {entities:?}"
        );
    }
}
