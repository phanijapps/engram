//! Stdio JSON-RPC 2.0 transport + per-message dispatch.
//!
//! The line loop is thin I/O glue; all dispatch logic lives in the pure,
//! unit-tested [`read_request`] / [`handle_request`] functions so the MCP
//! method routing is verifiable without driving stdin. Mirrors the
//! `codegraph/mcp-server` notification-skip pattern (a notification yields no
//! response). Generic over the handler context `C` threaded through to tools.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::protocol;
use crate::registry::{CallOutcome, ToolRegistry};

/// A response to write, or nothing (notifications carry no reply).
enum Response {
    Result(Value),
    Error(i64, String),
}

/// Parse one stdin line into a request. `None` for malformed JSON (skip).
fn read_request(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

/// Route a parsed request to its MCP method. `None` for notifications.
fn handle_request<C>(registry: &ToolRegistry<C>, ctx: &C, req: &Value) -> Option<Response> {
    let method = req["method"].as_str().unwrap_or("");
    let params = &req["params"];
    match method {
        "initialize" => Some(Response::Result(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "engram-mcp", "version": "0.1.0" }
        }))),
        "notifications/initialized" => None,
        "tools/list" => Some(Response::Result(json!({ "tools": registry.list() }))),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = &params["arguments"];
            match registry.call(ctx, name, args) {
                CallOutcome::Ok(result) => Some(Response::Result(result)),
                // Handler-reported failure: surface its code/message verbatim.
                CallOutcome::Err(err) => Some(Response::Error(err.code, err.message)),
                // Unknown tool name under a known method → invalid params.
                CallOutcome::NotFound => {
                    Some(Response::Error(-32602, format!("tool not found: {name}")))
                }
            }
        }
        // Unknown method. (`-32601` per JSON-RPC 2.0; distinct from the
        // `-32602` used for an unknown tool name under `tools/call`.)
        _ => Some(Response::Error(
            -32601,
            format!("method not found: {method}"),
        )),
    }
}

/// Run the stdio JSON-RPC loop until stdin closes or the client goes away.
pub fn run<C>(registry: ToolRegistry<C>, ctx: &C) {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { continue };
        let Some(req) = read_request(&line) else {
            continue;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);

        let Some(response) = handle_request(&registry, ctx, &req) else {
            continue;
        };

        let message = match response {
            Response::Result(result) => protocol::success(&id, result),
            Response::Error(code, message) => protocol::error(&id, code, &message),
        };
        // A write/flush error (e.g. broken pipe when the client dies) ends the
        // session cleanly rather than panicking.
        if writeln!(out, "{message}")
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{ToolError, ToolRecord, ToolRegistry};
    use serde_json::json;

    fn ping(_ctx: &(), _args: &Value) -> Result<Value, ToolError> {
        Ok(protocol::text_content("pong"))
    }

    fn boom(_ctx: &(), _args: &Value) -> Result<Value, ToolError> {
        Err(ToolError::new(-32603, "kaboom"))
    }

    fn registry_with_tools() -> ToolRegistry<()> {
        let mut r = ToolRegistry::new();
        r.register(ToolRecord {
            name: "ping",
            description: "ping",
            input_schema: json!({}),
            handler: ping,
        });
        r.register(ToolRecord {
            name: "boom",
            description: "boom",
            input_schema: json!({}),
            handler: boom,
        });
        r
    }

    #[test]
    fn read_request_rejects_malformed() {
        assert!(read_request("not json").is_none());
        assert!(read_request("{\"method\":\"ping\"}").is_some());
    }

    #[test]
    fn initialize_returns_server_info() {
        let resp =
            handle_request(&registry_with_tools(), &(), &json!({"method":"initialize"})).unwrap();
        match resp {
            Response::Result(v) => {
                assert_eq!(v["serverInfo"]["name"], "engram-mcp");
                assert_eq!(v["protocolVersion"], "2024-11-05");
                assert!(v["capabilities"]["tools"].is_object());
            }
            _ => panic!("expected result"),
        }
    }

    #[test]
    fn notification_is_skipped() {
        assert!(
            handle_request(
                &registry_with_tools(),
                &(),
                &json!({"method":"notifications/initialized"})
            )
            .is_none()
        );
    }

    #[test]
    fn tools_list_reflects_registry() {
        let resp =
            handle_request(&registry_with_tools(), &(), &json!({"method":"tools/list"})).unwrap();
        match resp {
            Response::Result(v) => {
                let names: Vec<&str> = v["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|t| t["name"].as_str())
                    .collect();
                assert_eq!(names, vec!["ping", "boom"]);
            }
            _ => panic!("expected result"),
        }
    }

    #[test]
    fn tools_call_dispatches() {
        let req = json!({"method":"tools/call","params":{"name":"ping","arguments":{}}});
        match handle_request(&registry_with_tools(), &(), &req).unwrap() {
            Response::Result(v) => assert_eq!(v["content"][0]["text"], "pong"),
            _ => panic!("expected result"),
        }
    }

    #[test]
    fn tools_call_surfaces_handler_error() {
        let req = json!({"method":"tools/call","params":{"name":"boom","arguments":{}}});
        match handle_request(&registry_with_tools(), &(), &req).unwrap() {
            Response::Error(code, msg) => {
                assert_eq!(code, -32603);
                assert_eq!(msg, "kaboom");
            }
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn unknown_tool_is_invalid_params() {
        let req = json!({"method":"tools/call","params":{"name":"nope","arguments":{}}});
        match handle_request(&registry_with_tools(), &(), &req).unwrap() {
            Response::Error(code, _) => assert_eq!(code, -32602),
            _ => panic!("expected error"),
        }
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        match handle_request(&registry_with_tools(), &(), &json!({"method":"frobnicate"})).unwrap()
        {
            Response::Error(code, _) => assert_eq!(code, -32601),
            _ => panic!("expected error"),
        }
    }
}
