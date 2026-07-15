//! Engram Memory MCP server.
//!
//! Exposes engram's memory operations (write_memory, recall) as MCP tools over
//! stdio JSON-RPC 2.0. Agents (Claude Code, Cursor) spawn this binary as a
//! subprocess and call the tools to use engram as a persistent memory layer.
//!
//! Usage: `engram-memory-mcp <storage-path>`

use std::io::{self, BufRead, Write};

use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityKind, EntityRef, ForgetRequest, Id,
    KnowledgeEntity, KnowledgeRelationship, MemoryContent, MemoryKind, Policy, Provenance,
    Retention, RetrievalRequest, Scope, Sensitivity, Visibility, WriteMemoryRequest,
};
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
};
use futures::executor::block_on;
use serde_json::{Value, json};

fn main() {
    let storage_path = std::env::args()
        .nth(1)
        .expect("usage: engram-memory-mcp <storage-path>");

    let config = EngramConfig::new(
        std::path::PathBuf::from(&storage_path),
        std::env::temp_dir(),
        engram_domain::ScopeMappingStrategy::Strict,
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
    let provider = EngramProvider::open(&config).expect("failed to open engram provider");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = req["method"].as_str().unwrap_or("");
        let id = req.get("id").cloned().unwrap_or(Value::Null);

        let resp = match method {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "engram-memory-mcp", "version": "0.1.0" }
                }
            }),
            "notifications/initialized" => continue,
            "tools/list" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "tools": tools_list() }
            }),
            "tools/call" => {
                let params = &req["params"];
                let name = params["name"].as_str().unwrap_or("");
                let args = &params["arguments"];
                let text = handle_tool(name, args, &provider);
                json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": text }] }
                })
            }
            _ => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": "method not found" }
            }),
        };
        writeln!(out, "{resp}").unwrap();
        out.flush().unwrap();
    }
}

fn tools_list() -> Vec<Value> {
    vec![
        json!({
            "name": "write_memory",
            "description": "Persist a fact, observation, or episode to engram's memory layer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "The memory content text." },
                    "tenant": { "type": "string", "description": "Tenant scope (default: 'default')." }
                },
                "required": ["content"]
            }
        }),
        json!({
            "name": "recall",
            "description": "Unified recall: one query fans across facts + graph + vector + lexical + beliefs, fused via RRF. Returns the most relevant context.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query." },
                    "tenant": { "type": "string", "description": "Tenant scope (default: 'default')." }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "forget",
            "description": "Delete, redact, or tombstone a memory by its target ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "The memory ID to forget." },
                    "mode": { "type": "string", "description": "delete | redact | tombstone | archive (default: tombstone)" },
                    "tenant": { "type": "string" }
                },
                "required": ["target_id"]
            }
        }),
        json!({
            "name": "put_entity",
            "description": "Add an entity to the knowledge graph.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "kind": { "type": "string", "description": "Person, Organization, Project, Concept, etc." },
                    "tenant": { "type": "string" }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "put_relationship",
            "description": "Add a relationship between two entities in the knowledge graph.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "subject": { "type": "string" },
                    "predicate": { "type": "string", "description": "works_at, owns, related_to, etc." },
                    "object": { "type": "string" },
                    "tenant": { "type": "string" }
                },
                "required": ["subject", "predicate", "object"]
            }
        }),
    ]
}

fn handle_tool(name: &str, args: &Value, provider: &EngramProvider) -> String {
    match name {
        "write_memory" => handle_write_memory(args, provider),
        "recall" => handle_recall(args, provider),
        "forget" => handle_forget(args, provider),
        "put_entity" => handle_put_entity(args, provider),
        "put_relationship" => handle_put_relationship(args, provider),
        _ => format!("Unknown tool: {name}"),
    }
}

fn default_scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn system_actor() -> Actor {
    Actor {
        id: Id::from("engram-memory-mcp"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn default_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn handle_write_memory(args: &Value, provider: &EngramProvider) -> String {
    let content = args["content"].as_str().unwrap_or("");
    let tenant = args["tenant"].as_str().unwrap_or("default");
    let now = chrono::Utc::now();

    let memory = match provider.require_memory() {
        Ok(handle) => handle,
        Err(e) => return format!("Memory capability not available: {e}"),
    };

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
        scope: default_scope(tenant),
        requester: engram_domain::Requester {
            actor: system_actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        provenance: Provenance {
            source: "engram-memory-mcp".to_owned(),
            actor: system_actor(),
            observed_at: now,
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("mcp-write".to_owned()),
        },
        policy: default_policy(),
        links: Vec::new(),
        idempotency_key: None,
    };

    match block_on(memory.write_memory(request)) {
        Ok(response) => serde_json::to_string_pretty(&response).unwrap_or_else(|_| "OK".to_owned()),
        Err(e) => format!("Write failed: {e}"),
    }
}

fn handle_recall(args: &Value, provider: &EngramProvider) -> String {
    let query = args["query"].as_str().unwrap_or("");
    let tenant = args["tenant"].as_str().unwrap_or("default");

    let recall = match provider.require_recall() {
        Ok(handle) => handle,
        Err(e) => return format!("Recall capability not available: {e}"),
    };

    let request = RetrievalRequest {
        query: query.to_owned(),
        scope: default_scope(tenant),
        requester: engram_domain::Requester {
            actor: system_actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    };

    match block_on(recall.recall(request)) {
        Ok(payload) => {
            let items: Vec<&str> = payload.items.iter().map(|i| i.content.as_str()).collect();
            if items.is_empty() {
                "No results.".to_owned()
            } else {
                items.join("\n---\n")
            }
        }
        Err(e) => format!("Recall failed: {e}"),
    }
}

fn default_provenance() -> Provenance {
    Provenance {
        source: "engram-memory-mcp".to_owned(),
        actor: system_actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("mcp".to_owned()),
    }
}

fn handle_forget(args: &Value, provider: &EngramProvider) -> String {
    let target_id = args["target_id"].as_str().unwrap_or("");
    let tenant = args["tenant"].as_str().unwrap_or("default");
    let mode = match args["mode"].as_str().unwrap_or("tombstone") {
        "delete" => DeleteMode::Delete,
        "redact" => DeleteMode::Redact,
        "archive" => DeleteMode::Archive,
        _ => DeleteMode::Tombstone,
    };
    let memory = match provider.require_memory() {
        Ok(h) => h,
        Err(e) => return format!("Memory not available: {e}"),
    };
    let request = ForgetRequest {
        target_type: engram_domain::ForgetTargetType::Memory,
        target_id: target_id.to_owned(),
        scope: default_scope(tenant),
        requester: engram_domain::Requester {
            actor: system_actor(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        mode,
        reason: None,
    };
    match block_on(memory.forget(request)) {
        Ok(r) => serde_json::to_string_pretty(&r).unwrap_or_else(|_| "OK".to_owned()),
        Err(e) => format!("Forget failed: {e}"),
    }
}

fn handle_put_entity(args: &Value, provider: &EngramProvider) -> String {
    let name = args["name"].as_str().unwrap_or("");
    let tenant = args["tenant"].as_str().unwrap_or("default");
    let knowledge = match provider.require_knowledge() {
        Ok(h) => h,
        Err(e) => return format!("Knowledge not available: {e}"),
    };
    let entity = KnowledgeEntity {
        id: Id::from(name),
        graph_id: None,
        kind: EntityKind::Concept,
        name: name.to_owned(),
        aliases: Vec::new(),
        scope: default_scope(tenant),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: default_provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    };
    match block_on(knowledge.put_entity(entity)) {
        Ok(e) => format!("Entity '{}' stored.", e.name),
        Err(e) => format!("Put entity failed: {e}"),
    }
}

fn handle_put_relationship(args: &Value, provider: &EngramProvider) -> String {
    let subject = args["subject"].as_str().unwrap_or("");
    let predicate = args["predicate"].as_str().unwrap_or("");
    let object = args["object"].as_str().unwrap_or("");
    let tenant = args["tenant"].as_str().unwrap_or("default");
    let knowledge = match provider.require_knowledge() {
        Ok(h) => h,
        Err(e) => return format!("Knowledge not available: {e}"),
    };
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
        scope: default_scope(tenant),
        evidence: Vec::new(),
        confidence: None,
        provenance: default_provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    };
    match block_on(knowledge.put_relationship(rel)) {
        Ok(_) => format!("{subject} -[{predicate}]-> {object} stored."),
        Err(e) => format!("Put relationship failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_provider() -> (TempDir, EngramProvider) {
        let dir = TempDir::new().expect("tempdir");
        let config = EngramConfig::new(
            dir.path().to_path_buf(),
            std::env::temp_dir(),
            engram_domain::ScopeMappingStrategy::Strict,
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
        (dir, provider)
    }

    #[test]
    fn write_then_recall_round_trips() {
        let (_dir, provider) = open_provider();

        let write_args = json!({ "content": "Alice works at Acme Corp", "tenant": "test" });
        let write_result = handle_write_memory(&write_args, &provider);
        assert!(
            !write_result.starts_with("Write failed"),
            "write should succeed: {write_result}"
        );

        let recall_args = json!({ "query": "Alice", "tenant": "test" });
        let recall_result = handle_recall(&recall_args, &provider);
        assert!(
            recall_result.contains("Alice") || recall_result.contains("Acme"),
            "recall should find the written memory: {recall_result}"
        );
    }
}
