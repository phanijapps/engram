//! Tool registry — the single source of truth for the MCP tool surface.
//!
//! `tools/list` and (Phase 3) `capability_report` both read from this registry,
//! so the advertised tool set cannot drift from the implemented one. Phase 1
//! keeps handlers synchronous and provider-less; Phase 2 evolves the handler
//! signature to carry the [`EngramProvider`] once tools need it.
//!
//! [`EngramProvider`]: engram_integration::EngramProvider

use serde_json::{Value, json};

/// A tool handler. Given the parsed `arguments` object, returns the MCP
/// `result` value to place under the JSON-RPC `result` field.
pub type ToolHandler = fn(args: &Value) -> Value;

/// One registered tool: its identity, schema, and handler.
pub struct ToolRecord {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: ToolHandler,
}

/// Outcome of dispatching a `tools/call` to the registry.
pub enum CallOutcome {
    /// The tool ran; the value is its MCP `result`.
    Ok(Value),
    /// No tool is registered under that name.
    NotFound,
}

/// The set of tools this server exposes.
pub struct ToolRegistry {
    tools: Vec<ToolRecord>,
}

impl ToolRegistry {
    /// An empty registry. `main` (via `register_all`) populates it.
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool. Registration order is preserved in `tools/list`.
    pub fn register(&mut self, tool: ToolRecord) {
        self.tools.push(tool);
    }

    /// The `tools/list` payload: one entry per registered tool.
    pub fn list(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect()
    }

    /// Dispatch a `tools/call` by name.
    pub fn call(&self, name: &str, args: &Value) -> CallOutcome {
        match self.tools.iter().find(|t| t.name == name) {
            Some(tool) => CallOutcome::Ok((tool.handler)(args)),
            None => CallOutcome::NotFound,
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo(args: &Value) -> Value {
        json!({ "content": [{ "type": "text", "text": args.to_string() }] })
    }

    fn reg() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        r.register(ToolRecord {
            name: "echo",
            description: "echo",
            input_schema: json!({}),
            handler: echo,
        });
        r
    }

    #[test]
    fn empty_registry_lists_nothing() {
        assert!(ToolRegistry::new().list().is_empty());
    }

    #[test]
    fn list_preserves_registration_order_and_shape() {
        let list = reg().list();
        let names: Vec<&str> = list.iter().filter_map(|v| v["name"].as_str()).collect();
        assert_eq!(names, vec!["echo"]);
        assert!(list[0]["inputSchema"].is_object());
    }

    #[test]
    fn call_dispatches_to_handler() {
        match reg().call("echo", &json!({ "x": 1 })) {
            CallOutcome::Ok(v) => assert!(
                v["content"][0]["text"].as_str().unwrap().contains("\"x\""),
                "echo should echo its args"
            ),
            CallOutcome::NotFound => panic!("expected dispatch"),
        }
    }

    #[test]
    fn call_unknown_is_not_found() {
        assert!(matches!(
            reg().call("missing", &json!({})),
            CallOutcome::NotFound
        ));
    }
}
