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

/// `code_staleness`: truthful freshness for the project's code graph. Reads
/// the latest checkpoint; git facts are local-only (no fetch).
pub fn code_staleness(app: &App, args: &Value) -> Result<Value, ToolError> {
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

/// R5: visible staleness line for codegraph answers. Cheap (a checkpoint
/// read plus local git); omitted when no checkpoint exists, because
/// unindexed repos have nothing to be stale against.
pub fn staleness_note(app: &App) -> String {
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
