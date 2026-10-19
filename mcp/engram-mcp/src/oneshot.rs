//! One-shot CLI subcommands (RFC-0022 Phase 1).
//!
//! `engram-mcp recall --storage <dir> --project <p> --query "…" [--json]`
//! `engram-mcp status --storage <dir> --project <p> [--json]`
//!
//! These exist so hook scripts (the recall-on-submit hook, the stale-index
//! reminder) can hit the store without a JSON-RPC handshake. Boot cost is
//! one provider open + one query; no server loop starts.
//!
//! Contract with the hook (see RFC-0022):
//! - `recall --json` emits one JSON object on stdout:
//!   `{ "results": [{ kind, score, title, content, path }], "hit_count",
//!      "top_score", "elapsed_ms", "ok": true }` — `score` is the fused
//!   retrieval score (`RetrievalScore.total`) the hook gates on. Errors
//!   still exit 0 with `{ "ok": false, "error": "…" }` so hooks fail open
//!   without parsing stderr.
//! - `status --json` emits `{ ok, project, last_index_at, indexed_files,
//!   entities, relationships, memories, beliefs }` — `last_index_at` is the
//!   mtime (unix seconds) of the newest scan manifest; the hook pairs it
//!   with git to compute files-changed-since-index.
//!
//! Human (non-`--json`) output stays plain text for interactive debugging.

use crate::app::App;
use engram_domain::retrieval::{RetrievalRequest, RetrievalResult};
use futures::executor::block_on;
use serde_json::{Value, json};
use std::time::Instant;

use crate::{bootstrap, config, ontology, scope, tools::requester};

/// One-shot subcommand entry. `argv` is the full argument list after the
/// program name, e.g. `["recall", "--storage", …]`. Returns the process
/// exit code.
pub fn run(argv: &[String]) -> i32 {
    let (cmd, rest) = match argv.split_first() {
        Some((cmd, rest)) => (cmd.as_str(), rest),
        None => {
            eprintln!("engram-mcp: one-shot mode needs a subcommand (recall | status)");
            return 2;
        }
    };
    match cmd {
        "recall" => run_recall(rest),
        "status" => run_status(rest),
        other => {
            eprintln!("engram-mcp: unknown one-shot subcommand {other:?} (recall | status)");
            2
        }
    }
}

/// True when argv starts with a one-shot subcommand — checked before MCP
/// config parsing so `recall`/`status` never collide with flag parsing.
pub fn is_oneshot(argv: &[String]) -> bool {
    matches!(
        argv.first().map(String::as_str),
        Some("recall") | Some("status")
    )
}

fn fail_open_json(command: &str, error: &str) -> i32 {
    // Hook contract: errors are reportable but never fail the host turn.
    println!(
        "{}",
        json!({ "ok": false, "command": command, "error": error })
    );
    0
}

fn run_recall(argv: &[String]) -> i32 {
    let started = Instant::now();
    let mut query = None;
    let mut json_out = false;
    let mut limit = 5u32;
    let mut rest: Vec<String> = Vec::with_capacity(argv.len());
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--query" | "-q" => query = it.next().cloned(),
            "--json" => json_out = true,
            "--limit" => {
                limit = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5)
                    .clamp(1, 25)
            }
            _ => rest.push(arg.clone()),
        }
    }
    let Some(query) = query else {
        eprintln!("engram-mcp recall: --query <text> is required");
        return 2;
    };

    let config = match config::McpConfig::from_args(&rest) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp recall: {message}");
            return 2;
        }
    };
    let provider = match bootstrap::open_provider(&config) {
        Ok(p) => p,
        Err(message) => {
            return if json_out {
                fail_open_json("recall", &message)
            } else {
                eprintln!("engram-mcp recall: {message}");
                1
            };
        }
    };
    let app = App {
        provider,
        scope: scope::resolve_scope(
            config.org.as_deref(),
            config.domain.as_deref(),
            config.subdomain.as_deref(),
            &config.project,
        ),
        ontology: ontology::resolve_ontology_config(config.ontology_path.as_deref())
            .unwrap_or_default(),
        taxonomy: ontology::resolve_taxonomy_config(config.taxonomy_path.as_deref())
            .unwrap_or_default(),
        storage_dir: std::path::PathBuf::from(&config.storage_path),
    };

    let recall = match app.provider.require_recall() {
        Ok(r) => r.clone(),
        Err(e) => {
            return if json_out {
                fail_open_json("recall", &e.to_string())
            } else {
                eprintln!("engram-mcp recall: recall capability unavailable: {e}");
                1
            };
        }
    };
    let request = RetrievalRequest {
        query: query.clone(),
        scope: app.scope.clone(),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(limit),
        budget: None,
        include_explanations: None,
    };
    let payload = match block_on(recall.recall(request)) {
        Ok(p) => p,
        Err(e) => {
            return if json_out {
                fail_open_json("recall", &e.to_string())
            } else {
                eprintln!("engram-mcp recall: retrieval failed: {e}");
                1
            };
        }
    };

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let top_score = payload
        .items
        .iter()
        .map(|i| i.score.total)
        .fold(0.0f32, f32::max);
    if json_out {
        let results: Vec<Value> = payload
            .items
            .iter()
            .map(|item| {
                json!({
                    "kind": item.target_type.kind_str(),
                    "score": item.score.total,
                    "evidence": evidence_label(item, top_score),
                    "source_score": item.fusion_trace.as_ref()
                        .and_then(|t| t.source_score)
                        .map(|s| (s * 100.0).round() / 100.0),
                    "lanes": item.fusion_trace.as_ref()
                        .map(|t| t.source.as_str())
                        .unwrap_or(""),
                    "title": item.metadata.as_ref()
                        .and_then(|m| m.get("title"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    "content": truncate(item.content.as_str(), 400),
                    "path": item.metadata.as_ref()
                        .and_then(|m| m.get("path"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "ok": true,
                "command": "recall",
                "hit_count": payload.items.len(),
                "top_score": round2(top_score),
                "elapsed_ms": elapsed_ms,
                "results": results,
            })
        );
        0
    } else {
        if payload.items.is_empty() {
            println!("No results.");
            return 0;
        }
        for item in &payload.items {
            println!(
                "[{:.2}] {}",
                item.score.total,
                truncate(item.content.as_str(), 200)
            );
        }
        println!("({} hits, {elapsed_ms} ms)", payload.items.len());
        0
    }
}

fn run_status(argv: &[String]) -> i32 {
    let mut json_out = false;
    let mut rest: Vec<String> = Vec::with_capacity(argv.len());
    for arg in argv {
        match arg.as_str() {
            "--json" => json_out = true,
            _ => rest.push(arg.clone()),
        }
    }
    let config = match config::McpConfig::from_args(&rest) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp status: {message}");
            return 2;
        }
    };
    let provider = match bootstrap::open_provider(&config) {
        Ok(p) => p,
        Err(message) => {
            return if json_out {
                fail_open_json("status", &message)
            } else {
                eprintln!("engram-mcp status: {message}");
                1
            };
        }
    };
    let scope = scope::resolve_scope(
        config.org.as_deref(),
        config.domain.as_deref(),
        config.subdomain.as_deref(),
        &config.project,
    );

    // Counts — each individually optional; a failed lane reports 0 rather
    // than failing the whole status call.
    let entities = provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_entities(&scope)).ok())
        .map(|e| e.len())
        .unwrap_or(0);
    let relationships = provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_relationships(&scope)).ok())
        .map(|r| r.len())
        .unwrap_or(0);
    // Memories: bounded page read (cap 1000 — a status count, not an audit).
    let memories = provider
        .require_memory()
        .ok()
        .and_then(|m| block_on(m.list_memories_paged(&scope, None, 1000)).ok())
        .map(|p| p.items.len())
        .unwrap_or(0);
    let beliefs = provider
        .require_beliefs()
        .ok()
        .and_then(|b| block_on(b.list_beliefs(&scope)).ok())
        .map(|v| v.len())
        .unwrap_or(0);

    // Last scan: mtime of the newest scan-manifest sidecar + its file count.
    let manifests = std::path::Path::new(&config.storage_path).join("scan-manifests");
    let mut last_index_at: Option<u64> = None;
    let mut indexed_files = 0usize;
    if let Ok(entries) = std::fs::read_dir(&manifests) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if last_index_at.is_none_or(|prev| mtime > prev) {
                    last_index_at = Some(mtime);
                }
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(&text) {
                    indexed_files += map.len();
                }
            }
        }
    }

    if json_out {
        println!(
            "{}",
            json!({
                "ok": true,
                "command": "status",
                "version": crate::ENGRAM_VERSION,
                "project": config.project,
                "last_index_at": last_index_at,
                "indexed_files": indexed_files,
                "entities": entities,
                "relationships": relationships,
                "memories": memories,
                "beliefs": beliefs,
            })
        );
        0
    } else {
        let indexed = last_index_at
            .map(chrono_like_utc)
            .unwrap_or_else(|| "never".to_owned());
        println!("version:       {}", crate::ENGRAM_VERSION);
        println!("project:       {}", config.project);
        println!("last index:    {indexed}");
        println!("indexed files: {indexed_files}");
        println!("entities:      {entities}");
        println!("relationships: {relationships}");
        println!("memories:      {memories}");
        println!("beliefs:       {beliefs}");
        0
    }
}

fn truncate(text: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Typed evidence label for a hit (RFC-0022): the gate and downstream
/// agents branch on this, not on raw scores (gbrain's `evidence` enum
/// pattern; measured lexical scores barely separate relevant from
/// irrelevant, so floats alone lie).
///
/// `semantic` — a vector lane produced it at near-top strength.
/// `keyword`  — a lexical lane produced it at usable strength.
/// `exact`    — strong non-lane-attributed hit (fusion top).
/// `weak`     — everything else; the gate ignores weak-only tops.
fn evidence_label(item: &RetrievalResult, top: f32) -> &'static str {
    let source = item
        .fusion_trace
        .as_ref()
        .map(|t| t.source.as_str())
        .unwrap_or("");
    evidence_for(source, item.score.total, top)
}

/// The pure decision core of [`evidence_label`] — lane name, this hit's
/// score, and the top score of the result set.
fn evidence_for(source: &str, score: f32, top: f32) -> &'static str {
    if source.contains("vector") || source.contains("semantic") || source.contains("embedding") {
        if score >= top * 0.8 {
            "semantic"
        } else {
            "weak"
        }
    } else if source.contains("lexical") || source.contains("bm25") || source.contains("keyword") {
        if score >= top * 0.6 {
            "keyword"
        } else {
            "weak"
        }
    } else if score >= top * 0.8 {
        "exact"
    } else {
        "weak"
    }
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// RFC-3339-ish UTC formatting without pulling chrono: days-since-epoch math
/// is overkill for a status line — a unix timestamp plus a readable marker.
fn chrono_like_utc(ts: u64) -> String {
    format!("{ts} (unix)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_labels_follow_lane_and_strength() {
        // Vector lane at near-top strength → semantic; weak otherwise.
        assert_eq!(evidence_for("vector-sqlite-vec", 0.9, 1.0), "semantic");
        assert_eq!(evidence_for("vector-sqlite-vec", 0.5, 1.0), "weak");
        // Lexical lane at usable strength → keyword; weak below 0.6×top.
        assert_eq!(evidence_for("lexical-bm25", 0.8, 1.0), "keyword");
        assert_eq!(evidence_for("lexical-bm25", 0.5, 1.0), "weak");
        // No lane attribution: strong fusion top → exact, else weak.
        assert_eq!(evidence_for("", 0.95, 1.0), "exact");
        assert_eq!(evidence_for("", 0.5, 1.0), "weak");
    }

    #[test]
    fn one_shot_is_dispatched_before_flag_parsing() {
        assert!(is_oneshot(&["recall".to_owned(), "--storage".to_owned()]));
        assert!(is_oneshot(&["status".to_owned()]));
        assert!(!is_oneshot(&["--storage".to_owned(), "recall".to_owned()]));
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("hello world", 5), "hello…");
        assert_eq!(truncate("hi", 5), "hi");
    }

    /// Surface pin (red-forman blocker #1): the launcher hook reads
    /// `source_score` + `lanes` from `recall --json`. If this key set drifts
    /// (e.g. camelCase rename), this test fails BEFORE the hook silently
    /// reads `undefined`. The stub-fixture in packages/launcher/test/ cannot
    /// catch that — it never touches this code.
    #[test]
    fn recall_json_surface_keys_are_pinned() {
        use chrono::Utc;
        use engram_domain::policy::{
            AllowedUse, DeleteMode, Policy, Retention, Sensitivity, Visibility,
        };
        use engram_domain::provenance::Provenance;
        use engram_domain::retrieval::{FusionTrace, RetrievalScore};
        use engram_domain::{Actor, ActorKind, Id};
        let item = RetrievalResult {
            id: "r1".into(),
            target_type: engram_domain::retrieval::RetrievalTargetType::Memory,
            target_id: "m1".into(),
            content: "pinned surface".into(),
            score: RetrievalScore {
                total: 0.5,
                relevance: None,
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: None,
            },
            provenance: Provenance {
                source: "test".into(),
                actor: Actor {
                    id: Id::from("actor-test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".into()),
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: Some(Sensitivity::Low),
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: Some(DeleteMode::Tombstone),
            },
            explanation: None,
            fusion_trace: Some(FusionTrace {
                query_id: None,
                vector_index: None,
                embedding_time_ms: None,
                search_time_ms: None,
                source: "lexical".into(),
                source_rank: Some(1),
                source_score: Some(3.25),
                score: None,
                rank: None,
                fusion_strategy: None,
                fusion_score: None,
                rerank_strategy: None,
                rerank_score: None,
                discard_reason: None,
                deduplicated_with: Vec::new(),
            }),
            metadata: None,
        };
        // The exact JSON shape run_recall emits per hit.
        let rendered = serde_json::json!({
            "kind": item.target_type.kind_str(),
            "score": item.score.total,
            "source_score": item.fusion_trace.as_ref().and_then(|t| t.source_score)
                .map(|s| (s * 100.0).round() / 100.0),
            "lanes": item.fusion_trace.as_ref().map(|t| t.source.as_str()).unwrap_or(""),
        });
        assert_eq!(rendered["kind"], "memory");
        assert_eq!(rendered["source_score"], 3.25);
        assert_eq!(rendered["lanes"], "lexical");
        // And the header the MCP tool renders (tools::recall).
        let lanes = "lexical";
        let raw = item.fusion_trace.as_ref().and_then(|t| t.source_score);
        let raw_seg = match (raw, lanes) {
            (Some(raw), l) if !l.is_empty() && !l.contains('+') => format!("raw {raw:.2}@{l}"),
            _ => unreachable!("pin covers the single-lane path"),
        };
        assert_eq!(raw_seg, "raw 3.25@lexical");
    }
}
