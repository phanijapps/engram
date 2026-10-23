//! Knowledge change log + scan freshness contracts (enhancements adoption,
//! slice 3).
//!
//! The change log is INVALIDATION METADATA, not recallable knowledge: no
//! document bodies or secrets in change records, and reads are scoped. A
//! resumable cursor lets long-lived consumers refresh without rescanning;
//! scan checkpoints carry truthful freshness (full revision OIDs, not
//! database mtimes). Sequence numbers are log-local ordering only — not
//! wall-clock, not git commit counts, not portable across engines.

use serde::{Deserialize, Serialize};

use crate::{Id, Scope, Timestamp};

/// What happened to a knowledge target. Minimal vocabulary; no-op writes do
/// not log events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeChangeOp {
    Upsert,
    Remove,
    Archive,
    Restore,
}

impl KnowledgeChangeOp {
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeChangeOp::Upsert => "upsert",
            KnowledgeChangeOp::Remove => "remove",
            KnowledgeChangeOp::Archive => "archive",
            KnowledgeChangeOp::Restore => "restore",
        }
    }
}

/// The knowledge target kinds that appear in the change log. Memories and
/// proposals never appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeChangeTarget {
    Graph,
    Entity,
    Relationship,
    Document,
    Chunk,
}

impl KnowledgeChangeTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeChangeTarget::Graph => "graph",
            KnowledgeChangeTarget::Entity => "entity",
            KnowledgeChangeTarget::Relationship => "relationship",
            KnowledgeChangeTarget::Document => "document",
            KnowledgeChangeTarget::Chunk => "chunk",
        }
    }
}

/// One committed knowledge mutation. `seq` is a decimal STRING on JSON
/// surfaces (SQLite 64-bit rows exceed JS's 53-bit integer range); cursor
/// interpretation stays opaque to SDK callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChange {
    /// Log-local sequence number, serialized as a decimal string.
    pub seq: String,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_source_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<Id>,
    pub target_type: KnowledgeChangeTarget,
    pub target_id: Id,
    pub op: KnowledgeChangeOp,
    pub occurred_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_run_id: Option<String>,
}

/// Resumable, scoped page request over the change log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChangesRequest {
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    /// Opaque cursor: the last consumed item's `seq` string; `None` starts
    /// at the oldest retained row. Cursors from another store generation or
    /// filter are rejected/reset explicitly, never silently reinterpreted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub limit: u32,
}

/// One bounded page of changes plus resume state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChangesPage {
    pub items: Vec<KnowledgeChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_watermark: Option<String>,
    pub has_more: bool,
    /// True when the cursor predates the log's retained window (retention
    /// pruning or a store rebuild): consumers must take a fresh snapshot
    /// instead of assuming an empty gap.
    pub reset_required: bool,
}

/// Truthful scan state for one (scope, stable source key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCheckpoint {
    pub scope: Scope,
    /// Stable source key (ADR-0017); the portable identity of the repo.
    pub stable_source_key: String,
    /// Host-local scan root path (diagnostic only; NOT portable identity).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_root: Option<String>,
    pub scan_run_id: String,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    /// Full git OID of the indexed revision (never a short SHA, never a
    /// database mtime).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_revision: Option<String>,
    pub status: ScanStatus,
    /// Worktree was dirty during the scan (indexed content is not exactly
    /// `indexed_revision` even when commits-behind is 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    pub errors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    /// Scan in flight (or interrupted without a terminal record).
    InProgress,
    /// Fully successful scan.
    Completed,
    /// Attempted scan with errors; last-known-good revision retained.
    Partial,
}

impl ScanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanStatus::InProgress => "in_progress",
            ScanStatus::Completed => "completed",
            ScanStatus::Partial => "partial",
        }
    }
}

/// Freshness of a code graph relative to its repository, computed at
/// response time (never cached past invalidation — a startup-only value
/// would itself go stale under a watcher).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeGraphStaleness {
    pub source_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_successful_scan_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    /// Commits between indexed and current revision (local, no fetch);
    /// `None` when unknown — never a fabricated 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commits_behind: Option<u64>,
    pub status: StalenessStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    pub scan_in_progress: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StalenessStatus {
    /// Indexed revision == current HEAD, clean scan.
    Current,
    /// Indexed revision is an ancestor of current HEAD.
    Behind,
    /// Git facts unavailable (no git, detached/unborn/shallow history, root
    /// gone) — explicit unknown, never guessed.
    Unknown,
    /// No successful checkpoint exists yet.
    Unindexed,
    /// Indexed revision is NOT an ancestor of current HEAD (rebase, reset).
    Diverged,
    /// Last scan attempt had errors.
    Partial,
}

impl StalenessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StalenessStatus::Current => "current",
            StalenessStatus::Behind => "behind",
            StalenessStatus::Unknown => "unknown",
            StalenessStatus::Unindexed => "unindexed",
            StalenessStatus::Diverged => "diverged",
            StalenessStatus::Partial => "partial",
        }
    }
}

/// Pure staleness derivation from a checkpoint plus observed git facts
/// (`current_revision` full OID; `commits_behind` from a local
/// `rev-list --count`, `None` when not computable; `dirty` from the
/// worktree). No fabricated zeros: unknown stays unknown.
pub fn derive_staleness(
    checkpoint: Option<&ScanCheckpoint>,
    source_key: &str,
    current_revision: Option<&str>,
    commits_behind: Option<u64>,
    dirty: Option<bool>,
) -> CodeGraphStaleness {
    let Some(cp) = checkpoint else {
        return CodeGraphStaleness {
            source_key: source_key.to_owned(),
            last_successful_scan_at: None,
            indexed_revision: None,
            current_revision: current_revision.map(str::to_owned),
            commits_behind: None,
            status: StalenessStatus::Unindexed,
            dirty,
            scan_in_progress: false,
            diagnostic: Some("no successful scan checkpoint recorded".to_owned()),
        };
    };
    let scan_in_progress = cp.status == ScanStatus::InProgress;
    let status = if cp.status == ScanStatus::Partial {
        StalenessStatus::Partial
    } else {
        match (cp.indexed_revision.as_deref(), current_revision) {
            (Some(indexed), Some(current)) => {
                if indexed == current {
                    if dirty.unwrap_or(false) {
                        StalenessStatus::Behind // dirty tree: not current
                    } else {
                        StalenessStatus::Current
                    }
                } else if commits_behind.unwrap_or(0) > 0 {
                    StalenessStatus::Behind
                } else {
                    StalenessStatus::Diverged
                }
            }
            (Some(_), None) => StalenessStatus::Unknown,
            (None, _) => StalenessStatus::Unknown,
        }
    };
    CodeGraphStaleness {
        source_key: source_key.to_owned(),
        last_successful_scan_at: if cp.status == ScanStatus::Completed {
            cp.completed_at
        } else {
            None
        },
        indexed_revision: cp.indexed_revision.clone(),
        current_revision: current_revision.map(str::to_owned),
        commits_behind,
        status,
        dirty: dirty.or(cp.dirty),
        scan_in_progress,
        diagnostic: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_seq_serializes_as_decimal_string() {
        let change = KnowledgeChange {
            seq: "9223372036854775807".to_owned(),
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            stable_source_key: Some("repo".into()),
            graph_id: None,
            target_type: KnowledgeChangeTarget::Relationship,
            target_id: Id::from("r-1"),
            op: KnowledgeChangeOp::Upsert,
            occurred_at: chrono::Utc::now(),
            scan_run_id: Some("run-1".into()),
        };
        let json = serde_json::to_string(&change).expect("serialize");
        assert!(json.contains("\"seq\":\"9223372036854775807\""), "{json}");
        let back: KnowledgeChange = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, change);
    }

    #[test]
    fn reset_required_survives_empty_pages() {
        let page = KnowledgeChangesPage {
            items: vec![],
            next_cursor: None,
            high_watermark: Some("42".into()),
            has_more: false,
            reset_required: true,
        };
        let json = serde_json::to_string(&page).expect("serialize");
        assert!(json.contains("\"resetRequired\":true"), "{json}");
    }
}
