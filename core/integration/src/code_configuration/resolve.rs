use engram_domain::CodeIntelligenceProfile;
use engram_retrieval::{CodeIntelligenceConfig, CodeIntelligenceConfigOverlay};
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCodeConfig {
    pub effective: CodeIntelligenceConfig,
    pub hash: String,
    pub origins: BTreeMap<String, String>,
}

pub(crate) fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}

fn merge(
    dst: &mut Value,
    src: Value,
    path: &str,
    origin: &str,
    origins: &mut BTreeMap<String, String>,
) {
    if let (Some(a), Some(b)) = (dst.as_object_mut(), src.as_object()) {
        for (k, v) in b {
            let field = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            merge(
                a.entry(k.clone()).or_insert(Value::Null),
                v.clone(),
                &field,
                origin,
                origins,
            );
        }
    } else {
        *dst = src;
        origins.insert(path.into(), origin.into());
    }
}

/// Profile is selected across all overlays before any fieldwise application.
pub fn resolve_code_config(
    file: Option<&CodeIntelligenceConfigOverlay>,
    environment: Option<&CodeIntelligenceConfigOverlay>,
    explicit: Option<&CodeIntelligenceConfigOverlay>,
) -> CoreResult<ResolvedCodeConfig> {
    let overlays = [
        (file, "file"),
        (environment, "environment"),
        (explicit, "explicit"),
    ];
    let profile = overlays
        .iter()
        .filter_map(|(o, _)| o.and_then(|c| c.profile))
        .last()
        .unwrap_or(CodeIntelligenceProfile::BalancedV1);
    let mut value = serde_json::to_value(CodeIntelligenceConfig::for_profile(profile))
        .map_err(|e| invalid(e.to_string()))?;
    let mut origins = BTreeMap::new();
    // Enumerate defaults as fields instead of a single root object.
    fn defaults(v: &Value, p: &str, out: &mut BTreeMap<String, String>) {
        if let Some(o) = v.as_object() {
            for (k, x) in o {
                defaults(
                    x,
                    &if p.is_empty() {
                        k.clone()
                    } else {
                        format!("{p}.{k}")
                    },
                    out,
                );
            }
        } else {
            out.insert(p.into(), "profile".into());
        }
    }
    origins.clear();
    defaults(&value, "", &mut origins);
    for (overlay, origin) in overlays {
        if let Some(overlay) = overlay {
            let v = serde_json::to_value(overlay).map_err(|e| invalid(e.to_string()))?;
            merge(&mut value, v, "", origin, &mut origins);
        }
    }
    let effective: CodeIntelligenceConfig =
        serde_json::from_value(value).map_err(|e| invalid(format!("code configuration: {e}")))?;
    effective.validate().map_err(|e| match e {
        CoreError::InvalidRequest { reason } => {
            let sources: Vec<_> = origins
                .iter()
                .filter(|(key, _)| reason.contains(key.as_str()))
                .map(|(key, source)| format!("{key} from {source}"))
                .collect();
            invalid(format!("{reason} [{}]", sources.join(", ")))
        }
        other => other,
    })?;
    let canonical = serde_json::to_vec(&effective).map_err(|e| invalid(e.to_string()))?;
    let hash = Sha256::digest(canonical)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(ResolvedCodeConfig {
        effective,
        hash,
        origins,
    })
}
