use super::resolve::invalid;
use engram_retrieval::CodeIntelligenceConfigOverlay;
use engram_runtime::CoreResult;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(crate) fn parse(env: &BTreeMap<String, String>) -> CoreResult<CodeIntelligenceConfigOverlay> {
    let mut root = Map::new();
    if let Some(profile) = env.get("ENGRAM_CODE_PROFILE") {
        root.insert("profile".into(), Value::String(profile.clone()));
    }
    for (key, raw) in env {
        if key == "ENGRAM_CODE_CONFIG" || key == "ENGRAM_CODE_PROFILE" {
            continue;
        }
        if !key.starts_with("ENGRAM_CODE") {
            continue;
        }
        let suffix = key
            .strip_prefix("ENGRAM_CODE__")
            .ok_or_else(|| invalid(format!("unknown code environment key: {key}")))?;
        let parts: Vec<_> = suffix.split("__").map(str::to_ascii_lowercase).collect();
        if !(2..=3).contains(&parts.len())
            || parts.iter().any(String::is_empty)
            || (parts.len() == 3
                && !(parts[0] == "fusion"
                    && (parts[1] == "source_weights" || parts[1] == "rerank")))
        {
            return Err(invalid(format!("invalid code environment suffix: {key}")));
        }
        let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()));
        let mut node = &mut root;
        for part in &parts[..parts.len() - 1] {
            node = node
                .entry(part.clone())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| invalid(format!("conflicting code environment key: {key}")))?;
        }
        node.insert(parts.last().unwrap().clone(), value);
    }
    serde_json::from_value(Value::Object(root))
        .map_err(|e| invalid(format!("code environment: {e}")))
}
