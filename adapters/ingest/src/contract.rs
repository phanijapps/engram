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

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeRepository};
use serde::Deserialize;
use serde_json::Value as JsonValue;

use crate::{extractor::repo_entity_id, hash::content_hash};

// ── Minimal OpenAPI model ─────────────────────────────────────────────────────
//
// Only the fields we inspect are deserialized. `serde(default)` on each
// optional container means missing/null fields silently produce an empty
// value rather than a parse error.

#[derive(Debug, Deserialize)]
struct OpenApiDoc {
    /// OpenAPI 3.x version marker (e.g. "3.0.0").
    openapi: Option<String>,
    /// Swagger 2.x version marker (e.g. "2.0").
    swagger: Option<String>,
    /// Path template → path item map.
    #[serde(default)]
    paths: HashMap<String, PathItem>,
}

#[derive(Debug, Deserialize, Default)]
struct PathItem {
    #[serde(default)]
    get: Option<Operation>,
    #[serde(default)]
    post: Option<Operation>,
    #[serde(default)]
    put: Option<Operation>,
    #[serde(default)]
    delete: Option<Operation>,
    #[serde(default)]
    patch: Option<Operation>,
    #[serde(default)]
    head: Option<Operation>,
    #[serde(default)]
    options: Option<Operation>,
    #[serde(default)]
    trace: Option<Operation>,
}

#[derive(Debug, Deserialize, Default)]
struct Operation {
    #[serde(default)]
    summary: Option<String>,
    #[serde(rename = "requestBody", default)]
    request_body: Option<RequestBody>,
    #[serde(default)]
    responses: HashMap<String, ResponseEntry>,
}

#[derive(Debug, Deserialize)]
struct RequestBody {
    #[serde(default)]
    content: HashMap<String, JsonValue>,
}

#[derive(Debug, Deserialize)]
struct ResponseEntry {
    #[serde(default)]
    content: Option<HashMap<String, JsonValue>>,
}

// ── Parsed operation (internal model) ────────────────────────────────────────

/// One REST operation extracted from an OpenAPI document.
#[derive(Debug, Clone)]
pub struct ParsedOperation {
    /// HTTP method in uppercase (e.g. `"GET"`).
    pub method: String,
    /// Raw path as it appears in the document (e.g. `"/orders/{id}"`).
    pub path: String,
    /// Operation summary from the spec, if present.
    pub summary: Option<String>,
    /// Media types from the request body content map.
    pub request_media_types: Vec<String>,
    /// Media types from the response content maps (de-duplicated).
    pub response_media_types: Vec<String>,
    /// Stable contract key (e.g. `"GET /orders/{}"`).
    pub normalized_key: String,
}

// ── YAML safety guard ─────────────────────────────────────────────────────────

// Conservative limits for untrusted YAML from arbitrary repositories.
//
// Anchor/alias caps are single-digit: legitimate OpenAPI specs essentially
// never use YAML anchors (they use JSON Pointer `$ref` instead). A low cap
// bounds the amplification factor even for fat-base attacks where one large
// anchor is referenced many times.
//
// The flow-collection depth cap guards against stack overflows driven by
// deeply nested flow-style `[[[[...]]]` on a single line — a vector that
// bypasses leading-indent measurement entirely.
const YAML_MAX_ANCHORS: usize = 4; // anchor definitions (&name)
const YAML_MAX_ALIASES: usize = 4; // alias references (*name)
const YAML_MAX_INDENT: usize = 128; // leading-whitespace chars → ~64 block levels
const YAML_MAX_FLOW_DEPTH: usize = 128; // running [{…}] depth across whole doc
const YAML_MAX_LINE_BYTES: usize = 64 * 1024; // per-line cap: closes the whole single-line-bomb family

/// Cheap pre-scan that rejects YAML inputs with pathological anchor/alias
/// density, excessive block-nesting depth, or excessive flow-collection depth
/// before handing control to the full serde_yml parser.
///
/// Vectors closed by this guard:
/// - Billion-laughs (deeply nested aliases): caught by alias count cap.
/// - Fat-base expansion (one large anchor × many aliases): caught by alias cap.
/// - Flow-nested stack overflow (`[[[[...]]]` on one line, indent = 0): caught
///   by the running `[`/`{` depth counter.
/// - Compact block-sequence chains (`- - - - x` on one line = nested block seqs
///   with zero leading indent and no `[`/`{`): caught by the per-line byte cap,
///   which closes the entire single-line-bomb family in one check.
///
/// Returns `Err(reason)` on a suspicious input; `Ok(())` when the text looks
/// safe to parse. False positives (legitimate spec with many `*`/`[`/`{` in
/// values) are acceptable — the file is skipped with a warning, not crashed.
fn check_yaml_safety(text: &str) -> Result<(), String> {
    let mut anchors: usize = 0;
    let mut aliases: usize = 0;
    // Running flow-collection depth tracked across all lines so single-line
    // deeply-nested flow docs are caught regardless of leading indent.
    let mut flow_depth: usize = 0;
    for (i, line) in text.lines().enumerate() {
        // Per-line byte cap: a single physical line longer than this can only be
        // a pathological single-line bomb (`[[[…`, `{{{…`, or compact `- - - …`
        // block sequences that recurse in serde_yml with zero leading indent).
        // Genuine multi-line block nesting stays bounded by YAML_MAX_INDENT.
        if line.len() > YAML_MAX_LINE_BYTES {
            return Err(format!(
                "YAML line {} length ({} bytes) exceeds safety limit ({YAML_MAX_LINE_BYTES})",
                i + 1,
                line.len()
            ));
        }
        // Compact block-sequence chains (`- - - … x`) nest one level per `- `
        // block-entry on a single physical line and carry no closing token to
        // track via flow_depth. Bound the per-line block-entry count to the same
        // depth cap, so all single-line nesting forms (`[`, `{`, `- `) are limited
        // to YAML_MAX_FLOW_DEPTH — keeping serde_yml's recursion well within a
        // rayon worker stack even for lines under the byte cap.
        let dash_entries = line.matches("- ").count();
        if dash_entries > YAML_MAX_FLOW_DEPTH {
            return Err(format!(
                "YAML compact block-sequence depth ({dash_entries}) exceeds safety limit \
                 ({YAML_MAX_FLOW_DEPTH}) at line {}",
                i + 1
            ));
        }
        // Count YAML anchor/alias sigils — rough proxy that covers billion-laughs
        // and fat-base alias patterns.
        anchors += line.chars().filter(|&c| c == '&').count();
        aliases += line.chars().filter(|&c| c == '*').count();
        if anchors > YAML_MAX_ANCHORS {
            return Err(format!(
                "YAML anchor count ({anchors}) exceeds safety limit ({YAML_MAX_ANCHORS}) \
                 at line {}",
                i + 1
            ));
        }
        if aliases > YAML_MAX_ALIASES {
            return Err(format!(
                "YAML alias count ({aliases}) exceeds safety limit ({YAML_MAX_ALIASES}) \
                 at line {}",
                i + 1
            ));
        }
        // Track flow-collection depth (running counter: `[`/`{` open, `]`/`}` close).
        // This catches `x: [[[[...]]]` bombs that have zero leading indent.
        for c in line.chars() {
            match c {
                '[' | '{' => {
                    flow_depth += 1;
                    if flow_depth > YAML_MAX_FLOW_DEPTH {
                        return Err(format!(
                            "YAML flow-collection depth ({flow_depth}) exceeds safety \
                             limit ({YAML_MAX_FLOW_DEPTH}) at line {}",
                            i + 1
                        ));
                    }
                }
                ']' | '}' => {
                    flow_depth = flow_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        // Block-nesting depth from leading whitespace.
        let indent = line.len() - line.trim_start().len();
        if indent > YAML_MAX_INDENT {
            return Err(format!(
                "YAML block-nesting depth (indent {indent}) exceeds safety limit \
                 ({YAML_MAX_INDENT} chars) at line {}",
                i + 1
            ));
        }
    }
    Ok(())
}

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

// ── Id derivation ─────────────────────────────────────────────────────────────

/// Derives a stable, document-independent `EntityId` for a contract node.
///
/// Keyed on the **full scope discriminator** + normalized key so two sources in
/// the same scope declaring the same operation resolve to the same entity id
/// (upsert converges to one node), while different scopes (tenants) never
/// collide.
pub(crate) fn contract_entity_id(scope: &Scope, normalized_key: &str) -> EntityId {
    Id::from(format!(
        "api-{}",
        content_hash(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{normalized_key}",
            scope.tenant,
            scope.subject.as_deref().unwrap_or(""),
            scope.workspace.as_deref().unwrap_or(""),
            scope.session.as_deref().unwrap_or(""),
            scope.environment.as_deref().unwrap_or(""),
        ))
        .trim_start_matches("sha256:")
    ))
}

/// Derives a stable `RelationshipId` for an `exposes` edge.
///
/// Keyed on `(stable_source_key, normalized_key)` — not on the ephemeral
/// `source_id` — so the edge id survives across commits of the same repo.
pub(crate) fn exposes_rel_id(stable_source_key: &str, normalized_key: &str) -> RelationshipId {
    Id::from(format!(
        "exposes-{}",
        content_hash(format!("{stable_source_key}\u{1f}{normalized_key}"))
            .trim_start_matches("sha256:")
    ))
}

// ── Entity + relationship construction ───────────────────────────────────────

/// Builds an `EntityKind::Api` entity for one REST operation.
///
/// The entity id is **document-independent** (keyed on scope + normalized key)
/// so upserts from different sources with the same operation converge to one
/// node. The `source_ref` uses `stable_source_key` (not the ephemeral source
/// id) so it is stable across commits.
pub(crate) fn build_api_entity(
    scope: &Scope,
    stable_source_key: &str,
    op: &ParsedOperation,
    provenance: &Provenance,
    now: DateTime<Utc>,
) -> KnowledgeEntity {
    let entity_id = contract_entity_id(scope, &op.normalized_key);

    let mut meta = Metadata::default();
    meta.insert("method".to_owned(), JsonValue::String(op.method.clone()));
    meta.insert("path".to_owned(), JsonValue::String(op.path.clone()));
    meta.insert(
        "normalizedKey".to_owned(),
        JsonValue::String(op.normalized_key.clone()),
    );
    if let Some(ref s) = op.summary {
        meta.insert("summary".to_owned(), JsonValue::String(s.clone()));
    }
    if !op.request_media_types.is_empty() {
        meta.insert(
            "requestMediaTypes".to_owned(),
            JsonValue::Array(
                op.request_media_types
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        );
    }
    if !op.response_media_types.is_empty() {
        meta.insert(
            "responseMediaTypes".to_owned(),
            JsonValue::Array(
                op.response_media_types
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        );
    }

    // source_ref: identifies the declaring repository by its stable key.
    // Using EvidenceTargetType::Source with target_id = stable_source_key so
    // the reference survives across commits (the stable key has no SHA).
    let source_ref = EvidenceRef {
        target_type: EvidenceTargetType::Source,
        target_id: Some(stable_source_key.to_owned()),
        uri: None,
        quote: None,
        location: None,
    };

    KnowledgeEntity {
        id: entity_id,
        graph_id: None, // not file-scoped; scope-global contract node
        kind: EntityKind::Api,
        name: op.normalized_key.clone(),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: vec![source_ref],
        concept_refs: Vec::new(),
        provenance: provenance.clone(),
        created_at: now,
        updated_at: None,
        metadata: Some(meta),
    }
}

/// Builds the `exposes` relationship from the Repository entity to the API entity.
///
/// The relationship id is derived from `(stable_source_key, normalized_key)`
/// so re-ingesting the same source (new commit) produces the same id and
/// upserts cleanly.
pub(crate) fn build_exposes_rel(
    scope: &Scope,
    stable_source_key: &str,
    op: &ParsedOperation,
    provenance: &Provenance,
    now: DateTime<Utc>,
) -> KnowledgeRelationship {
    let api_entity_id = contract_entity_id(scope, &op.normalized_key);
    let repo_id = repo_entity_id(scope, stable_source_key);

    KnowledgeRelationship {
        id: exposes_rel_id(stable_source_key, &op.normalized_key),
        graph_id: None, // not file-scoped
        subject: EntityRef {
            id: Some(repo_id),
            kind: Some("repository".to_owned()),
            name: Some(stable_source_key.to_owned()),
            aliases: Vec::new(),
        },
        predicate: "exposes".to_owned(),
        object: EntityRef {
            id: Some(api_entity_id),
            kind: Some("api".to_owned()),
            name: Some(op.normalized_key.clone()),
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(0.95),
        provenance: provenance.clone(),
        created_at: now,
        updated_at: None,
    }
}

// ── Source-ref union (T5 cross-repo merge) ────────────────────────────────────

/// Upserts an API entity with source-ref union (read-modify-write).
///
/// Reads any existing entity at the same id, unions the new source_ref into
/// the existing `source_refs` (idempotent: does not add a duplicate for the
/// same `stable_source_key`), then writes the merged entity back.
///
/// Safety guarantee rests on the **per-scope single-writer assumption** built
/// into this crate's ingest path: only one `scan_repository` call runs for a
/// given scope at a time. The SQLite adapter serializes individual store calls
/// through a `Mutex<Connection>`, but it does NOT hold that lock across the
/// read+write pair — the read and write are two separate lock acquisitions.
/// Concurrent ingests into the same scope from different processes could race
/// and lose a `source_ref`; that scenario is outside the supported operational
/// envelope for Phase A.
pub(crate) async fn upsert_api_entity_with_source_ref<R>(
    repo: &R,
    scope: &Scope,
    new_entity: KnowledgeEntity,
) -> CoreResult<()>
where
    R: KnowledgeRepository + ?Sized,
{
    let entity_id = new_entity.id.clone();
    // The new entity carries exactly one source_ref (this source's stable key).
    let new_ref = new_entity.source_refs.first().cloned();

    let merged = match repo.get_entity(&entity_id, scope).await? {
        Some(mut existing) => {
            // Union: only add if this source is not already in the list.
            if let Some(ref r) = new_ref {
                let already = existing
                    .source_refs
                    .iter()
                    .any(|e| e.target_id.as_deref() == r.target_id.as_deref());
                if !already {
                    existing.source_refs.push(r.clone());
                }
            }
            // Overwrite metadata with the latest parse (may have updated detail).
            existing.metadata = new_entity.metadata;
            existing
        }
        None => new_entity,
    };
    repo.put_entity(merged).await?;
    Ok(())
}

// ── Retraction (T8 per-source convergence) ───────────────────────────────────

/// Retracts one source's `exposes` edge and `source_ref` for a previously
/// declared contract key.
///
/// Steps:
/// 1. Delete the `exposes` relationship for `(stable_source_key, normalized_key)`.
/// 2. Read the API entity; remove the source_ref for `stable_source_key`.
/// 3. If `source_refs` is now empty, delete the entity (last source retracted).
///    Otherwise write back the trimmed entity.
///
/// Idempotent: if the relationship or entity is already absent, returns `Ok(())`.
pub(crate) async fn retract_contract_op<R>(
    repo: &R,
    scope: &Scope,
    stable_source_key: &str,
    normalized_key: &str,
) -> CoreResult<()>
where
    R: KnowledgeRepository + ?Sized,
{
    // Delete the exposes edge (ignore "not found" — idempotent).
    let rel_id = exposes_rel_id(stable_source_key, normalized_key);
    let _ = repo.delete_relationship(&rel_id, scope).await;

    // Remove this source's source_ref from the entity.
    let entity_id = contract_entity_id(scope, normalized_key);
    let Some(mut entity) = repo.get_entity(&entity_id, scope).await? else {
        return Ok(()); // already absent
    };
    entity
        .source_refs
        .retain(|r| r.target_id.as_deref() != Some(stable_source_key));

    if entity.source_refs.is_empty() {
        // Last source retracted — delete the orphaned contract node.
        let _ = repo.delete_entity(&entity_id, scope).await;
    } else {
        repo.put_entity(entity).await?;
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
