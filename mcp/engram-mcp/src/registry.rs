//! Tool registry — the single source of truth for the MCP tool surface.
//!
//! `tools/list` and (Phase 3) `capability_report` both read from this registry,
//! so the advertised tool set cannot drift from the implemented one. The
//! registry is generic over a handler context `C`: production passes the
//! [`EngramProvider`]; unit tests pass `()` so dispatch is verifiable without
//! opening a provider.
//!
//! [`EngramProvider`]: engram_integration::EngramProvider

use serde_json::{Value, json};

/// A tool handler. Given the handler context and the parsed `arguments`
/// object, returns the MCP `result` value to place under the JSON-RPC `result`
/// field.
pub type ToolHandler<C> = fn(ctx: &C, args: &Value) -> Value;

/// One registered tool: its identity, schema, and handler.
pub struct ToolRecord<C> {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: ToolHandler<C>,
}

/// Outcome of dispatching a `tools/call` to the registry.
pub enum CallOutcome {
    /// The tool ran; the value is its MCP `result`.
    Ok(Value),
    /// No tool is registered under that name.
    NotFound,
}

/// The set of tools this server exposes, keyed by handler context `C`.
pub struct ToolRegistry<C> {
    tools: Vec<ToolRecord<C>>,
}

impl<C> ToolRegistry<C> {
    /// An empty registry. `main` (via `register_all`) populates it.
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool. Registration order is preserved in `tools/list`.
    pub fn register(&mut self, tool: ToolRecord<C>) {
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
    pub fn call(&self, ctx: &C, name: &str, args: &Value) -> CallOutcome {
        match self.tools.iter().find(|t| t.name == name) {
            Some(tool) => CallOutcome::Ok((tool.handler)(ctx, args)),
            None => CallOutcome::NotFound,
        }
    }
}

impl<C> Default for ToolRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo(_ctx: &(), args: &Value) -> Value {
        json!({ "content": [{ "type": "text", "text": args.to_string() }] })
    }

    fn reg() -> ToolRegistry<()> {
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
        assert!(ToolRegistry::<()>::new().list().is_empty());
    }

    #[test]
    fn list_preserves_order_and_shape() {
        let list = reg().list();
        let names: Vec<&str> = list.iter().filter_map(|v| v["name"].as_str()).collect();
        assert_eq!(names, vec!["echo"]);
        assert!(list[0]["inputSchema"].is_object());
    }

    #[test]
    fn call_dispatches_to_handler() {
        match reg().call(&(), "echo", &json!({ "x": 1 })) {
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
            reg().call(&(), "missing", &json!({})),
            CallOutcome::NotFound
        ));
    }
}
