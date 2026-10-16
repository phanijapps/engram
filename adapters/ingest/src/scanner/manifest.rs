//! Versioned host sidecars accelerate scans; durable source state is authority.
use engram_domain::Scope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
struct ScanManifest {
    version: u32,
    fingerprint: String,
    scope: Scope,
    files: HashMap<String, String>,
}

pub fn decode_scan_manifest(text: &str, scope: &Scope) -> HashMap<String, String> {
    serde_json::from_str::<ScanManifest>(text)
        .ok()
        .filter(|m| {
            m.version == 1
                && m.fingerprint == super::source_assembly::FINGERPRINT
                && m.scope == *scope
        })
        .map(|m| m.files)
        .unwrap_or_default()
}
pub fn encode_scan_manifest(
    files: &HashMap<String, String>,
    scope: &Scope,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ScanManifest {
        version: 1,
        fingerprint: super::source_assembly::FINGERPRINT.into(),
        scope: scope.clone(),
        files: files.clone(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_version_and_fingerprint_are_bound() {
        let scope = Scope {
            tenant: "t".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        };
        let files = HashMap::from([("a.ts".into(), "hash".into())]);
        let text = encode_scan_manifest(&files, &scope).unwrap();
        assert_eq!(decode_scan_manifest(&text, &scope), files);
        let mut other = scope.clone();
        other.session = Some("s".into());
        assert!(decode_scan_manifest(&text, &other).is_empty());
        assert!(decode_scan_manifest(&text.replace("source-v1/", "old-v0/"), &scope).is_empty());
        assert!(decode_scan_manifest(r#"{"a.ts":"hash"}"#, &scope).is_empty());
    }
}
