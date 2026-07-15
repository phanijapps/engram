//! Engram Memory MCP server.
//!
//! Exposes engram's memory operations (write_memory, recall) as MCP tools over
//! stdio JSON-RPC 2.0. Agents (Claude Code, Cursor) spawn this binary as a
//! subprocess and call the tools to use engram as a persistent memory layer.
//!
//! Usage: `engram-memory-mcp <storage-path>`

use std::io::{self, BufRead, Write};

use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, Id, MemoryContent, MemoryKind, Policy, Provenance,
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
    ]
}

fn handle_tool(name: &str, args: &Value, provider: &EngramProvider) -> String {
    match name {
        "write_memory" => handle_write_memory(args, provider),
        "recall" => handle_recall(args, provider),
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
