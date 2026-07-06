//! OpenAPI contract detection, key normalization, and entity/edge extraction.
//!
//! Parses external OpenAPI v2/v3 documents discovered during repository
//! ingestion into a minimal model. No model or LLM calls — detection and
//! parsing are fully deterministic.
//!
//! # Roles
//! - `normalize_contract_key` — pure key derivation (T2).
//! - `detect_and_parse_openapi` — detection + operation parse (T3).
//! - `build_api_entity` / `build_exposes_rel` — entity + edge construction (T4).
//! - `upsert_api_entity_with_source_ref` — read-modify-write union (T5).
//! - `retract_contract_op` — source removal + orphan deletion (T8).
//!
//! # AC-6 / T7 conformance check (no model/LLM dep, no new crate boundary)
//!
//! Verify at any time with:
//! ```text
//! cargo tree -p engram-ingest --edges normal | grep -E 'openai|llm|embed|model'
//! ```
//! Expected output: empty (no matches). This module is fully deterministic.

use std::collections::HashSet;

#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use engram_domain::*;

use crate::{
    contract_entities::ParsedOperation,
    openapi_types::{OpenApiDoc, Operation, PathItem},
    yaml_safety::check_yaml_safety,
};

// Re-export entity construction functions for external use
pub use crate::contract_entities::{
    build_api_entity, build_exposes_rel, retract_contract_op, upsert_api_entity_with_source_ref,
};

// ── Detection + parsing ───────────────────────────────────────────────────────

/// Attempts to detect and parse an OpenAPI/Swagger document from text.
///
/// # Return values
/// - `Ok(None)` — no `openapi:`/`swagger:` version marker found; not an OpenAPI
///   document, no warning needed.
/// - `Ok(Some(ops))` — valid OpenAPI document; `ops` may be empty if the spec
///   has no paths.
/// - `Err(msg)` — the file has an OpenAPI marker but failed to parse (malformed
///   or truncated document), or failed the YAML safety check; the caller should
///   log `msg` and increment `ScanSummary.skipped`.
///
/// Only `.yaml`, `.yml`, and `.json` extensions are recognised. Any other
/// extension returns `Ok(None)`.
pub fn detect_and_parse_openapi(
    text: &str,
    ext: &str,
) -> Result<Option<Vec<ParsedOperation>>, String> {
    // Restrict to extensions we know how to parse.
    let is_yaml = matches!(ext, "yaml" | "yml");
    let is_json = ext == "json";
    if !is_yaml && !is_json {
        return Ok(None);
    }

    // Quick marker scan.
    //
    // YAML: check line-start markers in the first ~100 lines (the version field
    // is always near the top of a well-formed spec; avoids full parse cost for
    // non-OpenAPI YAML files).
    //
    // JSON: a minified document may place `"openapi":` anywhere on a single
    // line, so a line-start check would silently miss it. Use a substring search
    // instead (still cheaper than a full serde_json parse).
    let has_marker = if is_yaml {
        text.lines().take(100).any(|line| {
            let t = line.trim_start();
            t.starts_with("openapi:") || t.starts_with("swagger:")
        })
    } else {
        // JSON — handles both pretty-printed and minified documents.
        text.contains("\"openapi\"") || text.contains("\"swagger\"")
    };
    if !has_marker {
        return Ok(None);
    }

    // Full parse.  A marker was found, so a parse error means malformed spec.
    let doc: OpenApiDoc = if is_yaml {
        // Safety: reject inputs with pathological anchor/alias density before
        // handing to serde_yml (billion-laughs / stack-overflow guard).
        check_yaml_safety(text).map_err(|e| format!("OpenAPI YAML safety check failed: {e}"))?;
        serde_yml::from_str(text).map_err(|e| format!("OpenAPI YAML parse error: {e}"))?
    } else {
        serde_json::from_str(text).map_err(|e| format!("OpenAPI JSON parse error: {e}"))?
    };

    // Require the marker to appear as a root field (not just a substring).
    if doc.openapi.is_none() && doc.swagger.is_none() {
        return Ok(None);
    }

    Ok(Some(extract_operations(&doc)))
}

fn extract_operations(doc: &OpenApiDoc) -> Vec<ParsedOperation> {
    let mut ops = Vec::new();
    // Collect into a sorted list for deterministic test output.
    let mut paths: Vec<(&String, &PathItem)> = doc.paths.iter().collect();
    paths.sort_by_key(|(p, _)| *p);

    for (path, item) in paths {
        for (method, maybe_op) in method_slots(item) {
            let Some(op) = maybe_op else { continue };

            let req_types: Vec<String> = op
                .request_body
                .as_ref()
                .map(|rb| {
                    let mut keys: Vec<String> = rb.content.keys().cloned().collect();
                    keys.sort();
                    keys
                })
                .unwrap_or_default();

            let resp_types: HashSet<String> = op
                .responses
                .values()
                .filter_map(|rv| rv.content.as_ref())
                .flat_map(|c| c.keys().cloned())
                .collect();
            let mut resp_types: Vec<String> = resp_types.into_iter().collect();
            resp_types.sort();

            let normalized_key = normalize_contract_key(method, path);
            ops.push(ParsedOperation {
                method: method.to_owned(),
                path: path.clone(),
                summary: op.summary.clone(),
                request_media_types: req_types,
                response_media_types: resp_types,
                normalized_key,
            });
        }
    }
    ops
}

/// Returns `(method_name, &Option<Operation>)` for every HTTP method slot.
fn method_slots(item: &PathItem) -> [(&'static str, &Option<Operation>); 8] {
    [
        ("GET", &item.get),
        ("POST", &item.post),
        ("PUT", &item.put),
        ("DELETE", &item.delete),
        ("PATCH", &item.patch),
        ("HEAD", &item.head),
        ("OPTIONS", &item.options),
        ("TRACE", &item.trace),
    ]
}

// ── Key normalization ─────────────────────────────────────────────────────────

/// Derives the stable contract key for a REST operation.
///
/// Rules applied:
/// - Method is uppercased.
/// - Path parameters (`{name}`) are replaced with positional placeholders (`{}`).
/// - Trailing slashes are stripped (except for the root path `/`).
///
/// Two declarations of the same REST operation — even with different parameter
/// names — produce the same key, enabling cross-repo merge by upsert.
///
/// # Examples
///
/// ```
/// use engram_ingest::normalize_contract_key;
/// assert_eq!(normalize_contract_key("get", "/orders/{id}"), "GET /orders/{}");
/// assert_eq!(normalize_contract_key("GET", "/orders/{orderId}"), "GET /orders/{}");
/// assert_eq!(normalize_contract_key("POST", "/orders/"), "POST /orders");
/// assert_eq!(normalize_contract_key("get", "/"), "GET /");
/// ```
pub fn normalize_contract_key(method: &str, path: &str) -> String {
    let method = method.trim().to_uppercase();
    let path = normalize_path(path);
    format!("{method} {path}")
}

fn normalize_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            // Replace the entire `{…}` block with `{}`.
            result.push_str("{}");
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    // Strip trailing slash, but leave the root path `/` alone.
    if result.len() > 1 && result.ends_with('/') {
        result.pop();
    }
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_entities::contract_entity_id;

    // ── T2: normalize_contract_key ────────────────────────────────────────────

    #[test]
    fn normalizes_different_param_names_to_same_key() {
        assert_eq!(
            normalize_contract_key("GET", "/orders/{id}"),
            normalize_contract_key("GET", "/orders/{orderId}"),
            "path params with different names must fold to the same placeholder"
        );
    }

    #[test]
    fn normalized_key_matches_across_two_documents() {
        // Two documents may name the path parameter differently; the normalized
        // key must be byte-identical so cross-repo merge resolves to one entity.
        let key_a = normalize_contract_key("get", "/orders/{id}");
        let key_b = normalize_contract_key("GET", "/orders/{orderId}");
        assert_eq!(key_a, key_b);
        assert_eq!(key_a, "GET /orders/{}");
    }

    #[test]
    fn method_is_uppercased() {
        assert_eq!(normalize_contract_key("get", "/items"), "GET /items");
        assert_eq!(normalize_contract_key("post", "/items"), "POST /items");
    }

    #[test]
    fn trailing_slash_is_stripped() {
        assert_eq!(normalize_contract_key("GET", "/orders/"), "GET /orders");
    }

    #[test]
    fn root_path_slash_is_preserved() {
        assert_eq!(normalize_contract_key("get", "/"), "GET /");
    }

    #[test]
    fn nested_path_params_fold_to_positional_placeholders() {
        let key = normalize_contract_key("GET", "/repos/{owner}/{repo}/contents/{path}");
        assert_eq!(key, "GET /repos/{}/{}/contents/{}");
    }

    // ── T3: detect_and_parse_openapi ─────────────────────────────────────────

    const VALID_OPENAPI_YAML: &str = r#"
openapi: "3.0.0"
info:
  title: Orders API
  version: "1.0"
paths:
  /orders:
    post:
      summary: Create order
      requestBody:
        content:
          application/json: {}
      responses:
        "201":
          content:
            application/json: {}
  /orders/{id}:
    get:
      summary: Get order
      responses:
        "200":
          content:
            application/json: {}
"#;

    #[test]
    fn parses_valid_openapi_yaml_into_expected_operations() {
        let ops = detect_and_parse_openapi(VALID_OPENAPI_YAML, "yaml")
            .expect("should not be an error")
            .expect("should be OpenAPI");
        assert_eq!(ops.len(), 2, "expected 2 operations");

        let get_op = ops.iter().find(|o| o.method == "GET").expect("GET op");
        assert_eq!(get_op.normalized_key, "GET /orders/{}");
        assert_eq!(get_op.path, "/orders/{id}");
        assert_eq!(get_op.summary.as_deref(), Some("Get order"));

        let post_op = ops.iter().find(|o| o.method == "POST").expect("POST op");
        assert_eq!(post_op.normalized_key, "POST /orders");
        assert_eq!(
            post_op.request_media_types,
            vec!["application/json".to_owned()]
        );
    }

    #[test]
    fn non_openapi_yaml_is_not_treated_as_contract() {
        let generic_yaml = "name: my-service\nversion: 1.2.3\n";
        let result = detect_and_parse_openapi(generic_yaml, "yaml").expect("should not error");
        assert!(
            result.is_none(),
            "generic YAML must not be classified as OpenAPI"
        );
    }

    #[test]
    fn non_openapi_json_is_not_treated_as_contract() {
        let generic_json = r#"{"name":"foo","version":"1.0"}"#;
        let result = detect_and_parse_openapi(generic_json, "json").expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn unsupported_extension_returns_none() {
        let result =
            detect_and_parse_openapi(VALID_OPENAPI_YAML, "toml").expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn malformed_yaml_with_openapi_marker_returns_error() {
        // Has the marker as a line-start but the YAML is syntactically invalid.
        let malformed = "openapi: 3.0.0\npaths:\n  :\n    {invalid";
        let result = detect_and_parse_openapi(malformed, "yaml");
        assert!(
            result.is_err(),
            "malformed OpenAPI must be an error, not None"
        );
    }

    #[test]
    fn swagger2_marker_is_detected() {
        let swagger = r#"
swagger: "2.0"
info:
  title: Test
  version: "1.0"
paths:
  /ping:
    get:
      summary: ping
      responses:
        "200":
          description: ok
"#;
        let ops = detect_and_parse_openapi(swagger, "yaml")
            .expect("no error")
            .expect("swagger doc must be detected");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].normalized_key, "GET /ping");
    }

    // ── T4: entity / relationship construction ───────────────────────────────

    fn test_scope() -> Scope {
        Scope {
            tenant: "tenant-a".to_owned(),
            subject: None,
            workspace: Some("ws".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        }
    }

    fn test_provenance() -> Provenance {
        Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test-agent"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    #[test]
    fn api_entity_has_correct_kind_and_keying() {
        let scope = test_scope();
        let prov = test_provenance();
        let op = ParsedOperation {
            method: "GET".to_owned(),
            path: "/orders/{id}".to_owned(),
            summary: Some("Get order".to_owned()),
            request_media_types: Vec::new(),
            response_media_types: vec!["application/json".to_owned()],
            normalized_key: "GET /orders/{}".to_owned(),
        };
        let entity = build_api_entity(&scope, "github.com/acme/svc", &op, &prov, Utc::now());

        assert_eq!(entity.kind, EntityKind::Api);
        assert_eq!(entity.name, "GET /orders/{}");
        assert!(
            entity.graph_id.is_none(),
            "Api entity must not be file-scoped"
        );
        assert_eq!(entity.scope, scope);
        assert_eq!(entity.source_refs.len(), 1);
        assert_eq!(
            entity.source_refs[0].target_id.as_deref(),
            Some("github.com/acme/svc")
        );

        // AC-2: entity metadata must carry operation detail.
        let meta = entity.metadata.as_ref().expect("metadata must be present");
        assert_eq!(
            meta.get("method").and_then(|v| v.as_str()),
            Some("GET"),
            "metadata.method"
        );
        assert_eq!(
            meta.get("path").and_then(|v| v.as_str()),
            Some("/orders/{id}"),
            "metadata.path"
        );
        assert_eq!(
            meta.get("normalizedKey").and_then(|v| v.as_str()),
            Some("GET /orders/{}"),
            "metadata.normalizedKey"
        );
        assert_eq!(
            meta.get("summary").and_then(|v| v.as_str()),
            Some("Get order"),
            "metadata.summary"
        );
        assert!(
            meta.get("requestMediaTypes").is_none(),
            "metadata.requestMediaTypes must be absent when empty"
        );
        let resp_types = meta
            .get("responseMediaTypes")
            .and_then(|v| v.as_array())
            .expect("metadata.responseMediaTypes must be present");
        assert_eq!(
            resp_types
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>(),
            vec!["application/json"],
            "metadata.responseMediaTypes"
        );

        // Same op from a different doc in the same scope must produce the same id.
        let op2 = ParsedOperation {
            normalized_key: "GET /orders/{}".to_owned(),
            path: "/orders/{orderId}".to_owned(),
            ..op.clone()
        };
        let entity2 = build_api_entity(&scope, "github.com/acme/other", &op2, &prov, Utc::now());
        assert_eq!(
            entity.id, entity2.id,
            "same scope+key must produce the same entity id"
        );
    }

    #[test]
    fn exposes_rel_has_correct_predicate_and_confidence() {
        let scope = test_scope();
        let prov = test_provenance();
        let op = ParsedOperation {
            method: "POST".to_owned(),
            path: "/orders".to_owned(),
            summary: None,
            request_media_types: Vec::new(),
            response_media_types: Vec::new(),
            normalized_key: "POST /orders".to_owned(),
        };
        let rel = build_exposes_rel(&scope, "github.com/acme/svc", &op, &prov, Utc::now());

        assert_eq!(rel.predicate, "exposes");
        assert!(rel.confidence.is_some());
        assert!(rel.confidence.unwrap() > 0.0);
        assert!(
            rel.graph_id.is_none(),
            "exposes edge must not be file-scoped"
        );

        // id is stable across commits (derived from stable_source_key, not source_id)
        let rel2 = build_exposes_rel(&scope, "github.com/acme/svc", &op, &prov, Utc::now());
        assert_eq!(rel.id, rel2.id);
    }

    #[test]
    fn contract_entity_ids_differ_across_scopes() {
        let scope_a = Scope {
            tenant: "tenant-a".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        };
        let scope_b = Scope {
            tenant: "tenant-b".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        };
        let id_a = contract_entity_id(&scope_a, "GET /orders/{}");
        let id_b = contract_entity_id(&scope_b, "GET /orders/{}");
        assert_ne!(
            id_a, id_b,
            "different tenants must not share contract entity ids"
        );
    }

    // ── YAML safety guard ────────────────────────────────────────────────────

    #[test]
    fn yaml_bomb_is_rejected_as_malformed() {
        // A classic billion-laughs YAML bomb.  The many alias references far
        // exceed the single-digit alias budget; the safety check must catch it
        // and return Err — counted as "skipped", never crashes or OOMs.
        let bomb = "openapi: \"3.0.0\"\n\
                    a: &a [\"x\",\"x\"]\n\
                    b: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a,*a]\n\
                    c: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b,*b]\n\
                    d: &d [*c,*c,*c,*c,*c,*c,*c,*c,*c,*c]\n\
                    e: &e [*d,*d,*d,*d,*d,*d,*d,*d,*d,*d]\n\
                    paths: {}\n";
        let result = detect_and_parse_openapi(bomb, "yaml");
        assert!(
            result.is_err(),
            "YAML bomb must be rejected (Err), not parsed silently: {result:?}"
        );
    }

    #[test]
    fn flow_nested_yaml_is_rejected() {
        // Deeply nested FLOW-style collections on a single line carry indent 0,
        // so an indent-only guard would miss them; the running flow-depth counter
        // must reject them before serde_yml recurses into a stack-overflow abort().
        let doc = format!("openapi: \"3.0.0\"\nx: {}\npaths: {{}}\n", "[".repeat(300));
        let result = detect_and_parse_openapi(&doc, "yaml");
        assert!(
            result.is_err(),
            "flow-nested YAML must be rejected before parse: {result:?}"
        );
    }

    #[test]
    fn fat_base_yaml_is_rejected() {
        // One large anchored base scalar referenced by many aliases (fat-base
        // expansion); the alias count exceeds the single-digit budget → rejected
        // before serde_yml can expand it into an OOM.
        let big = "A".repeat(4096);
        let doc = format!(
            "openapi: \"3.0.0\"\nbase: &b \"{big}\"\nx: [*b,*b,*b,*b,*b,*b]\npaths: {{}}\n"
        );
        let result = detect_and_parse_openapi(&doc, "yaml");
        assert!(
            result.is_err(),
            "fat-base alias expansion must be rejected before parse: {result:?}"
        );
    }

    #[test]
    fn compact_block_seq_yaml_is_rejected() {
        // Compact nested block sequences (`- - - - x` on one line) recurse one
        // level per `-` in serde_yml but carry zero leading indent and no `[`/`{`
        // — bypassing the flow-depth and indent checks. At 30K entries (~60 KiB)
        // the line is UNDER the byte cap, so this exercises the per-line
        // block-entry (`- `) depth cap specifically, before a stack-overflow abort().
        let doc = format!(
            "openapi: \"3.0.0\"\nx: {}\npaths: {{}}\n",
            "- ".repeat(30_000)
        );
        let result = detect_and_parse_openapi(&doc, "yaml");
        assert!(
            result.is_err(),
            "compact block-sequence YAML must be rejected before parse: {result:?}"
        );
    }

    #[test]
    fn minified_json_openapi_is_detected() {
        // A minified single-line JSON OpenAPI document — the marker is not at
        // line-start, so the old line-scan check would miss it silently.
        let minified = r#"{"openapi":"3.0.0","info":{"title":"Mini","version":"1"},"paths":{"/ping":{"get":{"summary":"ping","responses":{"200":{"description":"ok"}}}}}}"#;
        let ops = detect_and_parse_openapi(minified, "json")
            .expect("no error")
            .expect("minified JSON OpenAPI must be detected");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].normalized_key, "GET /ping");
    }

    #[test]
    fn non_openapi_json_is_not_treated_as_contract_after_substring_fix() {
        // JSON without any "openapi" or "swagger" key must still return Ok(None).
        let plain_json = r#"{"name":"my-service","version":"1.0.0"}"#;
        let result = detect_and_parse_openapi(plain_json, "json").expect("no error");
        assert!(
            result.is_none(),
            "plain JSON must not be treated as OpenAPI"
        );
    }
}
