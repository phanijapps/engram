//! JSON-RPC 2.0 envelope helpers.

use serde_json::{Value, json};

/// A successful response: `{ jsonrpc, id, result }`.
pub fn success(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// An error response: `{ jsonrpc, id, error: { code, message } }`.
pub fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The standard MCP text-content result wrapper.
pub fn text_content(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_envelope() {
        let v = success(&json!(1), json!({ "ok": true }));
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 1);
        assert_eq!(v["result"]["ok"], true);
    }

    #[test]
    fn error_envelope() {
        let v = error(&json!(2), -32601, "nope");
        assert_eq!(v["id"], 2);
        assert_eq!(v["error"]["code"], -32601);
        assert_eq!(v["error"]["message"], "nope");
    }

    #[test]
    fn text_content_wraps() {
        let v = text_content("hi");
        assert_eq!(v["content"][0]["type"], "text");
        assert_eq!(v["content"][0]["text"], "hi");
    }
}
