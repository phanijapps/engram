//! Strict wire DTOs and trusted launch authority over the shared Rust operation.
use crate::{
    app::App,
    protocol,
    registry::ToolError,
    tools::{internal, invalid, requester},
};
use engram_domain::CodeEvidenceQuery;
use futures::executor::block_on;
use serde_json::Value;

pub fn evidence(app: &App, args: &Value) -> Result<Value, ToolError> {
    let mut wire = args
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("code evidence arguments must be an object"))?;
    wire.remove("op");
    let query: CodeEvidenceQuery =
        serde_json::from_value(Value::Object(wire)).map_err(|e| invalid(e.to_string()))?;
    let packet =
        block_on(app.provider.code_evidence(&query, &app.scope, &requester())).map_err(internal)?;
    Ok(protocol::text_content(
        serde_json::to_string(&packet).map_err(internal)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn strict_evidence_wire_rejects_authority_and_model_fields_before_provider_use() {
        let root = tempfile::tempdir().unwrap();
        let app = crate::tools::tests::test_app(root.path());
        for extra in [
            json!({"scope":{"tenant":"other"}}),
            json!({"requester":{}}),
            json!({"model":"remote"}),
            json!({"calibration":{}}),
            json!({"profile":"fast-v1"}),
            json!({"query":"é".repeat(8193)}),
        ] {
            let mut args = json!({"op":"evidence","query":"valid"});
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let error = evidence(&app, &args).unwrap_err();
            assert_eq!(error.code, -32602, "{}", error.message);
        }
        assert!(evidence(&app, &json!({"op":"evidence","query":"valid"})).is_err());
    }
}
