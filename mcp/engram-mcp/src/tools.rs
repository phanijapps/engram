//! MCP tool handlers that route through the [`EngramProvider`].
//!
//! Each handler borrows the shared [`App`] (provider + fused-per-project scope)
//! and returns [`Result<Value, ToolError>`] so failures surface as JSON-RPC
//! errors rather than being embedded in a success string. The domain-record
//! construction mirrors the proven `memory/mcp-server` pattern.

use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, ConsolidationRequest, DeleteMode, EntityKind, EntityRef,
    ForgetRequest, ForgetTargetType, Id, KnowledgeEntity, KnowledgeRelationship, MemoryContent,
    MemoryKind, Policy, Provenance, Requester, Retention, RetrievalRequest, RetrievalTargetType,
    Sensitivity, Visibility, WriteMemoryRequest,
};
use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;

// --- shared record helpers ----------------------------------------------------

fn system_actor() -> Actor {
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

fn policy() -> Policy {
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

fn internal(msg: impl std::fmt::Display) -> ToolError {
    ToolError::new(-32603, msg.to_string())
}

fn invalid(msg: impl std::fmt::Display) -> ToolError {
    ToolError::new(-32602, msg.to_string())
}

/// A required, non-empty string arg; `-32602` (invalid params) if absent/empty.
fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
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
}
