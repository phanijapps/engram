//! Protocol-boundary extraction (post-index scan).
//!
//! `scan_protocols` walks a repository AFTER `scan_repo` and extracts HTTP
//! protocol boundaries: client calls (`fetch`, `axios`, `this.get/post`) +
//! server routes (Axum `.route`, Express `app.get`). It normalizes route
//! patterns, creates `Api` endpoint entities, and connects callers → endpoints
//! → handlers so `change_impact` / `symbol_context` cross the protocol boundary.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use engram_domain::{EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship};
use futures::executor::block_on;
use regex::Regex;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, provenance, req_str};

/// `scan_protocols`: post-index scan that extracts HTTP protocol boundaries
/// (client calls + server routes), normalizes route patterns, creates `Api`
/// endpoint entities, and links callers → endpoints → handlers.
pub fn scan_protocols(app: &App, args: &Value) -> Result<Value, ToolError> {
    let root = req_str(args, "path")?;
    let files = walk_code_files(Path::new(root));
    if files.is_empty() {
        return Ok(protocol::text_content("No code files found."));
    }

    // Regex patterns.
    let axum_re =
        Regex::new(r#"\.route\(\s*"([^"]+)"\s*,\s*(get|post|put|delete|patch)\(\s*([a-z_:]+)"#)
            .map_err(|e| internal(format!("regex error: {e}")))?;
    let express_re = Regex::new(
        r#"(?:app|router)\.(get|post|put|delete|patch)\(\s*"([^"]+)"\s*,\s*([a-zA-Z_]+)"#,
    )
    .map_err(|e| internal(format!("regex error: {e}")))?;
    let fetch_re = Regex::new(r#"\bfetch(?:Json)?\s*\(\s*[`'"]([^`'"]+)[`'"]"#)
        .map_err(|e| internal(format!("regex error: {e}")))?;
    let client_method_re =
        Regex::new(r#"\.(get|post|put|delete|patch)\s*(?:<[^>]+>)?\s*\(\s*[`'"]([^`'"]+)[`'"]"#)
            .map_err(|e| internal(format!("regex error: {e}")))?;
    let route_param_re = Regex::new(r"(:[a-zA-Z_][a-zA-Z0-9_]*|\$\{[^}]+\}|\{[^}]+\})")
        .map_err(|e| internal(format!("regex error: {e}")))?;

    let knowledge = app.provider.require_knowledge().map_err(internal)?;
    let scope = app.scope.clone();
    let now = Utc::now();
    let prov = provenance("mcp-scan-protocols");

    // Collect endpoints: canonical_id → (method, normalized_path)
    let mut endpoints: HashMap<String, (String, String)> = HashMap::new();
    // Collect handler edges: endpoint_id → handler_name
    let mut handler_edges: Vec<(String, String)> = Vec::new();
    // Collect client edges: endpoint_id → caller_name
    let mut client_edges: Vec<(String, String)> = Vec::new();

    let mut files_scanned = 0usize;

    for (rel_path, text) in &files {
        files_scanned += 1;
        let is_test = rel_path.contains("/test")
            || rel_path.contains("/tests/")
            || rel_path.contains("/conformance")
            || rel_path.ends_with("_test.rs")
            || rel_path.ends_with(".test.ts")
            || rel_path.ends_with(".spec.ts");

        // --- Server routes ---
        for caps in axum_re.captures_iter(text) {
            let path = &caps[1];
            let method = &caps[2];
            let handler = &caps[3];
            let normalized = normalize(path, &route_param_re);
            let ep_id = endpoint_id(method, &normalized);
            endpoints
                .entry(ep_id.clone())
                .or_insert((method.to_owned(), normalized));
            handler_edges.push((ep_id, handler.trim_start_matches("()").to_owned()));
        }
        for caps in express_re.captures_iter(text) {
            let method = &caps[1];
            let path = &caps[2];
            let handler = &caps[3];
            let normalized = normalize(path, &route_param_re);
            let ep_id = endpoint_id(method, &normalized);
            endpoints
                .entry(ep_id.clone())
                .or_insert((method.to_owned(), normalized));
            handler_edges.push((ep_id, handler.to_owned()));
        }

        // --- Client calls (skip test files to avoid polluting the protocol graph) ---
        if is_test {
            continue;
        }
        for caps in fetch_re.captures_iter(text) {
            let url = &caps[1];
            if let Some(path_part) = extract_path(url) {
                let normalized = normalize(&path_part, &route_param_re);
                let ep_id = endpoint_id("get", &normalized);
                endpoints
                    .entry(ep_id.clone())
                    .or_insert(("get".to_owned(), normalized));
                if let Some(caller) =
                    enclosing_function(text, caps.get(0).map(|m| m.start()).unwrap_or(0))
                {
                    client_edges.push((ep_id, caller));
                }
            }
        }
        for caps in client_method_re.captures_iter(text) {
            let method = &caps[1];
            let url = &caps[2];
            if let Some(path_part) = extract_path(url) {
                let normalized = normalize(&path_part, &route_param_re);
                let ep_id = endpoint_id(method, &normalized);
                endpoints
                    .entry(ep_id.clone())
                    .or_insert((method.to_owned(), normalized));
                if let Some(caller) =
                    enclosing_function(text, caps.get(0).map(|m| m.start()).unwrap_or(0))
                {
                    client_edges.push((ep_id, caller));
                }
            }
        }
    }

    // --- Wrapper propagation ---
    // Functions that contain HTTP calls with DYNAMIC urls (e.g. fetch(`${base}${path}`))
    // are "wrappers" — their callers pass a static URL that we CAN extract.
    let mut wrapper_names: HashSet<String> = HashSet::new();
    for (_rel_path, text) in &files {
        for caps in fetch_re.captures_iter(text) {
            let url = &caps[1];
            let pos = caps.get(0).map(|m| m.start()).unwrap_or(0);
            if extract_path(url).is_none() {
                if let Some(func) = enclosing_function(text, pos) {
                    wrapper_names.insert(func);
                }
            }
        }
        for caps in client_method_re.captures_iter(text) {
            let url = &caps[2];
            let pos = caps.get(0).map(|m| m.start()).unwrap_or(0);
            if extract_path(url).is_none() {
                if let Some(func) = enclosing_function(text, pos) {
                    wrapper_names.insert(func);
                }
            }
        }
    }

    // For each caller of a wrapper that passes a static URL, create a
    // sends_request edge to the matching endpoint.
    if !wrapper_names.is_empty() {
        let escaped: Vec<String> = wrapper_names.iter().map(|n| regex::escape(n)).collect();
        let pattern = format!(
            r#"\b({})\s*(?:<[^>]+>)?\s*\(\s*[`'"]([^`'"]+)[`'"]"#,
            escaped.join("|")
        );
        if let Ok(wrapper_re) = Regex::new(&pattern) {
            for (_rel_path, text) in &files {
                for caps in wrapper_re.captures_iter(text) {
                    let wrapper_name = &caps[1];
                    let url = &caps[2];
                    let pos = caps.get(0).map(|m| m.start()).unwrap_or(0);
                    if let Some(path_part) = extract_path(url) {
                        let normalized = normalize(&path_part, &route_param_re);
                        for method in ["get", "post", "put", "delete", "patch"] {
                            let ep_id = endpoint_id(method, &normalized);
                            if endpoints.contains_key(&ep_id) {
                                if let Some(caller) = enclosing_function(text, pos) {
                                    if caller != wrapper_name {
                                        client_edges.push((ep_id, caller));
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Persist endpoint entities.
    let mut entity_count = 0;
    for (ep_id, (method, path)) in &endpoints {
        let entity = KnowledgeEntity {
            id: Id::from(ep_id.clone()),
            graph_id: None,
            kind: EntityKind::Api,
            name: format!("{method} {path}"),
            aliases: Vec::new(),
            scope: scope.clone(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        };
        block_on(knowledge.put_entity(entity)).map_err(internal)?;
        entity_count += 1;
    }

    // Persist handler edges: endpoint -[handled_by]-> handler
    for (ep_id, handler) in &handler_edges {
        let (method, path) = &endpoints[ep_id];
        let ep_name = format!("{method} {path}");
        let handler_short = handler.rsplit("::").next().unwrap_or(handler.as_str());
        let rel = KnowledgeRelationship {
            id: Id::from(format!("{ep_id}\u{1f}handled_by\u{1f}{handler}")),
            graph_id: None,
            subject: EntityRef {
                id: Some(Id::from(ep_id.clone())),
                kind: Some("api".to_owned()),
                name: Some(ep_name),
                aliases: Vec::new(),
            },
            predicate: "handled_by".to_owned(),
            object: EntityRef {
                id: None,
                kind: None,
                name: Some(handler_short.to_owned()),
                aliases: Vec::new(),
            },
            scope: scope.clone(),
            evidence: Vec::new(),
            confidence: Some(0.9),
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
        };
        block_on(knowledge.put_relationship(rel)).map_err(internal)?;
    }

    // Persist client edges: caller -[sends_request]-> endpoint
    for (ep_id, caller) in &client_edges {
        let (method, path) = &endpoints[ep_id];
        let ep_name = format!("{method} {path}");
        let rel = KnowledgeRelationship {
            id: Id::from(format!("{caller}\u{1f}sends_request\u{1f}{ep_id}")),
            graph_id: None,
            subject: EntityRef {
                id: None,
                kind: None,
                name: Some(caller.clone()),
                aliases: Vec::new(),
            },
            predicate: "sends_request".to_owned(),
            object: EntityRef {
                id: Some(Id::from(ep_id.clone())),
                kind: Some("api".to_owned()),
                name: Some(ep_name),
                aliases: Vec::new(),
            },
            scope: scope.clone(),
            evidence: Vec::new(),
            confidence: Some(0.8),
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
        };
        block_on(knowledge.put_relationship(rel)).map_err(internal)?;
    }

    Ok(protocol::text_content(format!(
        "Protocol scan: {files_scanned} files, {entity_count} HTTP endpoint entities, \
         {} handler links, {} client links. Use symbol_context / change_impact to traverse.",
        handler_edges.len(),
        client_edges.len()
    )))
}

// --- helpers ---

/// Normalize a route path so server declarations and client calls match.
/// Handles: `:param`, `${var}`, `{actual}`, numeric IDs, and long hashes.
fn normalize(path: &str, param_re: &Regex) -> String {
    let path = path.split('?').next().unwrap_or(path);
    let path = param_re.replace_all(path, "{param}");
    path.split('/')
        .map(|seg| {
            if !seg.is_empty()
                && (seg.chars().all(|c| c.is_ascii_digit())
                    || (seg.len() > 20
                        && seg
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')))
            {
                "{param}".to_owned()
            } else {
                seg.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Deterministic endpoint entity id from method + normalized path.
fn endpoint_id(method: &str, normalized_path: &str) -> String {
    format!("endpoint-{method}-{normalized_path}")
}

/// Extract the path portion from a URL (strip protocol/host, keep path).
fn extract_path(url: &str) -> Option<String> {
    let path = if url.starts_with("http://") || url.starts_with("https://") {
        url.splitn(4, '/').nth(3).map(|p| format!("/{p}"))?
    } else {
        url.to_owned()
    };
    if path.starts_with('/') {
        Some(path)
    } else {
        None
    }
}

/// Find the nearest enclosing function declaration above `pos` in `text`.
fn enclosing_function(text: &str, pos: usize) -> Option<String> {
    let before = &text[..pos];
    for line in before.lines().rev() {
        let trimmed = line.trim_start();
        for prefix in &[
            "fn ",
            "function ",
            "def ",
            "async fn ",
            "async function ",
            "pub fn ",
            "pub async fn ",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                if name.len() > 1 {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// Walk code files recursively, skipping hidden/vendor dirs.
fn walk_code_files(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    walk_dir(root, root, &mut files);
    files
}

fn walk_dir(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.')
                || matches!(
                    name,
                    "node_modules" | "target" | "dist" | "build" | "__pycache__" | "vendor"
                )
            {
                continue;
            }
            walk_dir(root, &path, files);
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(
                ext,
                "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "java" | "go" | "rb" | "cs"
            ) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if text.len() > 512 * 1024 {
                        continue; // skip very large files
                    }
                    let rel = path
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    files.push((rel, text));
                }
            }
        }
    }
}
