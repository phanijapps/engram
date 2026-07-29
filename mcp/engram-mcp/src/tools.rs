//! MCP tool handlers that route through the [`EngramProvider`].
//!
//! Each handler borrows the shared [`App`] (provider + fused-per-project scope)
//! and returns [`Result<Value, ToolError>`] so failures surface as JSON-RPC
//! errors rather than being embedded in a success string. The domain-record
//! construction mirrors the proven `memory/mcp-server` pattern.

use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityKind, EntityRef, ForgetRequest,
    ForgetTargetType, Id, KnowledgeEntity, KnowledgeRelationship, MemoryContent, MemoryKind,
    Policy, Provenance, Requester, Retention, Sensitivity, Visibility, WriteMemoryRequest,
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

/// Parse a free-form `kind` string into an [`EntityKind`], defaulting to
/// `Concept` for anything that isn't a known variant (so `put_entity` honors the
/// caller's kind without hard-coding `Concept`).
fn parse_entity_kind(kind: &str) -> EntityKind {
    serde_json::from_value(serde_json::Value::String(kind.to_owned()))
        .unwrap_or(EntityKind::Concept)
}

// --- tools --------------------------------------------------------------------

/// `write_memory`: persist an observation/episode to the memory layer.
pub fn write_memory(app: &App, args: &Value) -> Result<Value, ToolError> {
    let content = args["content"].as_str().unwrap_or("");
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
    let target_id = args["target_id"].as_str().unwrap_or("");
    let mode = match args["mode"].as_str().unwrap_or("tombstone") {
        "delete" => DeleteMode::Delete,
        "redact" => DeleteMode::Redact,
        "archive" => DeleteMode::Archive,
        _ => DeleteMode::Tombstone,
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

/// `put_entity`: add an entity to the knowledge graph (honoring the `kind` arg).
pub fn put_entity(app: &App, args: &Value) -> Result<Value, ToolError> {
    let name = args["name"].as_str().unwrap_or("");
    let kind = parse_entity_kind(args["kind"].as_str().unwrap_or("Concept"));
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
pub fn put_relationship(app: &App, args: &Value) -> Result<Value, ToolError> {
    let subject = args["subject"].as_str().unwrap_or("");
    let predicate = args["predicate"].as_str().unwrap_or("");
    let object = args["object"].as_str().unwrap_or("");
    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let rel = KnowledgeRelationship {
        id: Id::from(format!("{subject}-{predicate}-{object}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
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
    fn write_memory_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let res = write_memory(&app, &json!({ "content": "Alice works at Acme" }));
        assert!(res.is_ok(), "write_memory should succeed: {:?}", res.err());
    }

    #[test]
    fn put_entity_honors_kind_and_stores() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let res = put_entity(&app, &json!({ "name": "AuthService", "kind": "Service" }));
        assert!(res.is_ok(), "{:?}", res.err());
        // Unknown kind falls back to Concept, still Ok.
        let res2 = put_entity(&app, &json!({ "name": "Mystery", "kind": "NotARealKind" }));
        assert!(res2.is_ok(), "{:?}", res2.err());
    }

    #[test]
    fn put_relationship_stores() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let res = put_relationship(
            &app,
            &json!({ "subject": "AuthService", "predicate": "realized_by", "object": "auth.rs" }),
        );
        assert!(res.is_ok(), "{:?}", res.err());
    }
}
