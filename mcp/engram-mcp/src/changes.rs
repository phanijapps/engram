//! Change-log + freshness tools (enhancements adoption, slice 3).
//!
//! `subscribe_changes` is BOUNDED POLLING, not a stream: one page plus a
//! `nextCursor`; long-lived consumers poll. `code_staleness` reports the
//! truthful freshness of the code graph relative to its repository (full
//! OIDs, local ancestry only, never a fabricated zero). `staleness_note`
//! decorates codegraph answers with a visible `STALENESS:` line (R5).

use engram_domain::{KnowledgeChangesRequest, ScanCheckpoint, StalenessStatus};

use futures::executor::block_on;
use serde_json::Value;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::internal;

/// One bounded page of committed knowledge changes after a cursor.
pub fn subscribe_changes(app: &App, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"].as_u64().unwrap_or(50).min(500) as u32;
    let after = args["after"].as_str().map(str::to_owned);
    let source_key = args["source_key"].as_str().map(str::to_owned);
    let log = app.provider.require_change_log().map_err(internal)?;
    let page = block_on(log.changes_since(&KnowledgeChangesRequest {
        scope: app.scope.clone(),
        source_key,
        after,
        limit,
    }))
    .map_err(internal)?;
    let mut lines: Vec<String> = page
        .items
        .iter()
        .map(|c| {
            format!(
                "- seq={} {} {} {}{}",
                c.seq,
                c.op.as_str(),
                c.target_type.as_str(),
                c.target_id,
                c.stable_source_key
                    .as_deref()
                    .map(|k| format!(" source={k}"))
                    .unwrap_or_default()
            )
        })
        .collect();
    if page.reset_required {
        lines.push(
            "RESET REQUIRED: the cursor predates the retained log window — take a fresh snapshot; do not treat the gap as empty.".to_owned(),
        );
    }
    if lines.is_empty() {
        lines.push("No new changes.".to_owned());
    }
    let mut footer = format!(
        "hasMore={} highWatermark={}",
        page.has_more,
        page.high_watermark.as_deref().unwrap_or("-")
    );
    if let Some(cursor) = &page.next_cursor {
        footer.push_str(&format!(" nextCursor={cursor}"));
    }
    Ok(protocol::text_content(format!(
        "Knowledge changes (invalidation metadata — never recallable content):\n{}\n{footer}",
        lines.join("\n")
    )))
}

/// The scope's published bound repositories (empty = no bound code). This is
/// the review r6 finding 4 boundary: bound-code answers NEVER consult a
/// legacy checkpoint's checkout HEAD — when ANY repository is published, the
/// binding-keyed durable path is the only path.
fn published_bound_repos(app: &App) -> Vec<String> {
    app.provider
        .code_bindings()
        .ok()
        .and_then(|coordinator| {
            block_on(coordinator.store().eligibility(&app.scope))
                .ok()
                .map(|eligibility| eligibility.repository_ids)
        })
        .unwrap_or_default()
}

/// `code_staleness`: truthful freshness for the project's code graph.
///
/// TASK-018 + review r6 finding 4 — binding-keyed freshness: when `repo` (a
/// RepoId) names a BOUND repository, the answer comes from the DURABLE
/// binding record — the CONFIGURED ref, the indexed generation OID, and the
/// last on-request ref observation — and NEVER consults a checkout HEAD, the
/// worktree, or any scan-root checkpoint. With no `repo` and ANY published
/// repository in scope, the answer is the binding-keyed summary over the
/// published set (durable records only). The LEGACY checkpoint path below is
/// explicitly reserved for scopes with NO bound code and labeled as such.
pub fn code_staleness(app: &App, args: &Value) -> Result<Value, ToolError> {
    if let Some(repo) = args["repo"].as_str() {
        return binding_keyed_freshness(app, repo);
    }
    if !published_bound_repos(app).is_empty() {
        return bound_scope_freshness(app);
    }
    let log = app.provider.require_change_log().map_err(internal)?;
    // Explicit source_key wins; otherwise the FRESHEST checkpoint in this
    // scope (the tenant is not a source key — checkpoints are keyed by repo
    // identity, so the default resolves through the checkpoint list).
    let checkpoints = block_on(log.latest_checkpoints(&app.scope)).map_err(internal)?;
    let (key, checkpoint) = match args["source_key"].as_str() {
        Some(key) => {
            let cp = block_on(log.latest_checkpoint(&app.scope, key)).map_err(internal)?;
            (key.to_owned(), cp)
        }
        None => match checkpoints.first() {
            Some(cp) => (cp.stable_source_key.clone(), Some(cp.clone())),
            None => (app.scope.tenant.clone(), None),
        },
    };
    let (current, behind, dirty) = git_facts(checkpoint.as_ref());
    let staleness = engram_domain::derive_staleness(
        checkpoint.as_ref(),
        &key,
        current.as_deref(),
        behind,
        dirty,
    );
    Ok(protocol::text_content(render_staleness(&staleness)))
}

/// Local git facts for the checkpoint's scan root (no fetch, no origin
/// requirement). Returns (current_head, commits_behind, dirty).
pub fn git_facts(
    checkpoint: Option<&ScanCheckpoint>,
) -> (Option<String>, Option<u64>, Option<bool>) {
    use std::path::Path;
    let Some(cp) = checkpoint else {
        return (None, None, None);
    };
    let Some(root) = cp.scan_root.as_deref().map(Path::new) else {
        return (None, None, cp.dirty);
    };
    let current = engram_ingest::git::current_head(root);
    let behind = match (&current, cp.indexed_revision.as_deref()) {
        (Some(cur), Some(indexed)) if cur != indexed => {
            engram_ingest::git::commits_behind(root, indexed, cur)
        }
        (Some(_), Some(_)) => Some(0),
        _ => None,
    };
    let dirty = engram_ingest::git::worktree_dirty(root);
    (current, behind, dirty)
}

pub fn render_staleness(s: &engram_domain::CodeGraphStaleness) -> String {
    let mut out = format!(
        "STALENESS: status={} commits_behind={}",
        s.status.as_str(),
        s.commits_behind
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unknown".into())
    );
    if let Some(indexed) = &s.indexed_revision {
        out.push_str(&format!("\n  indexed:  {indexed}"));
    }
    if let Some(current) = &s.current_revision {
        out.push_str(&format!("\n  current:  {current}"));
    }
    if let Some(dirty) = s.dirty {
        out.push_str(&format!("\n  dirty worktree: {dirty}"));
    }
    if s.scan_in_progress {
        out.push_str("\n  scan in progress");
    }
    if let Some(at) = s.last_successful_scan_at {
        out.push_str(&format!("\n  last successful scan: {at}"));
    }
    if let Some(d) = &s.diagnostic {
        out.push_str(&format!("\n  note: {d}"));
    }
    out
}

/// R5: visible staleness line for codegraph answers. Review r6 finding 4:
/// when the scope has ANY published bound repository, the note is the
/// BINDING-KEYED durable summary (configured ref, indexed OID, on-request
/// observation) — an unrelated legacy scan checkpoint's HEAD NEVER leaks
/// into a bound scope's answer. The checkpoint path below serves only
/// UNBOUND legacy scopes.
pub fn staleness_note(app: &App) -> String {
    if !published_bound_repos(app).is_empty() {
        return bound_scope_freshness(app)
            .map(|value| Server::text_of_note(&value))
            .unwrap_or_default();
    }
    let Ok(log) = app.provider.require_change_log() else {
        return String::new();
    };
    let latest = block_on(log.latest_checkpoints(&app.scope))
        .ok()
        .and_then(|all| all.first().cloned());
    let Some(cp) = latest else {
        return String::new();
    };
    let (current, behind, dirty) = git_facts(Some(&cp));
    let staleness = engram_domain::derive_staleness(
        Some(&cp),
        &cp.stable_source_key,
        current.as_deref(),
        behind,
        dirty,
    );
    if staleness.status == StalenessStatus::Current {
        String::new()
    } else {
        format!(
            "\n{}",
            render_staleness(&staleness)
                .lines()
                .next()
                .unwrap_or("STALENESS: unknown")
        )
    }
}

/// TASK-018: the binding-keyed freshness answer for one bound repository.
/// Durable-record driven: configured ref, indexed generation OID, and the
/// last on-request ref observation with its policy and time. Checkout HEAD,
/// the worktree, and scan-root checkpoints are NEVER read — two repos whose
/// scan roots share a basename cannot select each other's metadata because
/// every fact is keyed by (scope, RepoId).
fn binding_keyed_freshness(app: &App, repo: &str) -> Result<Value, ToolError> {
    use engram_domain::RepoId;
    let coordinator = app.provider.code_bindings().map_err(internal)?;
    let requester = crate::tools::requester();
    let actor = requester.actor;
    let status = block_on(coordinator.status(&actor, &app.scope, &RepoId(repo.to_owned()), None))
        .map_err(internal)?;
    let mut out = format!(
        "FRESHNESS (binding-keyed, durable records only — never checkout HEAD):\n  repository: {}\n  configured ref: {}\n  policy: on_request\n  indexed: {}",
        status.checkpoint.repo_id.0,
        status.freshness.branch_ref,
        status
            .indexed_commit_oid
            .as_deref()
            .unwrap_or("none (no published generation or unreadable)"),
    );
    match (&status.freshness.observed_oid, status.freshness.observed_at) {
        (Some(oid), Some(at)) => out.push_str(&format!("\n  observed ref: {oid} at {at}")),
        _ => out.push_str("\n  observed ref: none"),
    }
    match status.freshness.state {
        engram_integration::code_binding::CodeFreshnessState::Unknown => {
            out.push_str("\n  freshness: unknown (no valid observation — run an explicit refresh)")
        }
        engram_integration::code_binding::CodeFreshnessState::Observed {
            matches_indexed: Some(true),
        } => out.push_str("\n  freshness: observed at the indexed commit"),
        engram_integration::code_binding::CodeFreshnessState::Observed {
            matches_indexed: Some(false),
        } => {
            out.push_str("\n  freshness: observed ref moved past the indexed commit (rebuild required to index it)")
        }
        engram_integration::code_binding::CodeFreshnessState::Observed {
            matches_indexed: None,
        } => out.push_str("\n  freshness: observed; indexed commit unreadable"),
    }
    out.push_str(&format!(
        "\n  binding version: {} (code epoch {})\n  checkpoint generation: {}",
        status.checkpoint.binding_version,
        status.checkpoint.code_epoch,
        status
            .checkpoint
            .generation
            .as_ref()
            .map(|generation| format!(
                "{} @ {} (parser {}, filter {})",
                generation.id,
                generation.commit_oid,
                generation.parser_fingerprint,
                generation.filter_fingerprint
            ))
            .unwrap_or_else(|| "none".to_owned()),
    ));
    Ok(protocol::text_content(out))
}

/// Renders the binding-keyed durable freshness summary for a scope with
/// PUBLISHED bound code (review r6 finding 4). Durable records only — the
/// configured ref, the indexed generation OID, and the last on-request
/// observation per repository; checkout HEAD, worktrees, and scan-root
/// checkpoints are never read.
fn bound_scope_freshness(app: &App) -> Result<Value, ToolError> {
    use engram_domain::RepoId;
    let coordinator = app.provider.code_bindings().map_err(internal)?;
    let actor = crate::tools::requester().actor;
    let mut out =
        String::from("FRESHNESS (binding-keyed, durable records only — never checkout HEAD):");
    let repos = published_bound_repos(app);
    if repos.is_empty() {
        out.push_str("\n  no published bound code in this scope");
        return Ok(protocol::text_content(out));
    }
    for repo in repos {
        let status = block_on(coordinator.status(&actor, &app.scope, &RepoId(repo.clone()), None))
            .map_err(internal)?;
        out.push_str(&format!(
            "\n  repository {} ({}): indexed {}",
            status.checkpoint.repo_id.0,
            status.freshness.branch_ref,
            status
                .indexed_commit_oid
                .as_deref()
                .unwrap_or("none (no published generation or unreadable)"),
        ));
        match status.freshness.state {
            engram_integration::code_binding::CodeFreshnessState::Unknown => {
                out.push_str("; freshness unknown (no valid observation)");
            }
            engram_integration::code_binding::CodeFreshnessState::Observed {
                matches_indexed: Some(true),
            } => out.push_str("; observed at the indexed commit"),
            engram_integration::code_binding::CodeFreshnessState::Observed {
                matches_indexed: Some(false),
            } => {
                out.push_str("; observed ref moved past the indexed commit");
            }
            engram_integration::code_binding::CodeFreshnessState::Observed {
                matches_indexed: None,
            } => out.push_str("; observed; indexed commit unreadable"),
        }
    }
    Ok(protocol::text_content(out))
}

/// The first line of a text_content payload (annotation rendering).
struct Server;
impl Server {
    fn text_of_note(value: &Value) -> String {
        value["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
