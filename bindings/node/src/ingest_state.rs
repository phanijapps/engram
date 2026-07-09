//! Scan job state management for the ingest engine.
//!
//! Internal state tracking for background repository scan jobs.

use serde::Serialize;

/// Snapshot of a background scan job, returned to Node as JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanJobState {
    pub status: String, // "running" | "done" | "error"
    pub current_file: Option<String>,
    pub processed: usize,
    pub ingested: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub errors: usize,
    pub summary: Option<engram_ingest::ScanSummary>,
    pub error: Option<String>,
}

impl ScanJobState {
    pub fn running() -> Self {
        Self {
            status: "running".to_owned(),
            current_file: None,
            processed: 0,
            ingested: 0,
            unchanged: 0,
            skipped: 0,
            errors: 0,
            summary: None,
            error: None,
        }
    }
}
