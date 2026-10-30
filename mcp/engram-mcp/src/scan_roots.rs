//! Scan-root registry: resolves the on-disk root a `scan_repo` ingested
//! from, so `fetch_content` can read verbatim source at call time.
//!
//! LEGACY / UNBOUND surface only (TASK-018 boundary note): a BOUND code
//! repository never resolves content through this sidecar — its exact
//! source is the retained immutable snapshot keyed by the durable binding
//! (scope + RepoId + generation). Root basenames and source strings here
//! are therefore never code authority, and basename collisions between two
//! different repositories cannot select a bound repository's metadata.
//!
//! Two layers:
//! - **In-memory** (process-local): every successful `scan_repo` records
//!   `provenance source → absolute root`. Same-process fetches resolve
//!   immediately.
//! - **Persisted sidecar** `<storage>/scan-roots.json`: `source → {root,
//!   scanned_at}`. Survives process restarts (fetch after a scan done by an
//!   earlier process). Missing/corrupt sidecar degrades to in-memory only —
//!   verbatim is a capability, never a hard dependency; `fetch_content`
//!   falls back to chunk stitching.
//!
//! Multiple roots sharing one source string (two non-git scans both stamp
//! `engram-mcp-scan`) collide by design: the LAST scan wins, matching the
//! store's own last-write-wins ingest semantics. `fetch_content` treats a
//! root mismatch (rel path does not exist under the resolved root) as
//! unresolvable and falls back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Process-local (storage_dir, source) → root map. The storage dir is part
/// of the key so concurrent tests (distinct temp storages, same source
/// string) and multi-store servers never cross-resolve roots.
static ROOTS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

fn key(storage: &Path, source: &str) -> String {
    format!("{}\u{0}{}", storage.to_string_lossy(), source)
}

fn sidecar_path(storage: &Path) -> PathBuf {
    storage.join("scan-roots.json")
}

/// Record that `source` was scanned from `root` (in-memory + sidecar).
/// Sidecar write failures are non-fatal (verbatim degrades, never breaks).
pub fn record(storage: &Path, source: &str, root: &Path) {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    {
        let mut guard = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get_or_insert_with(HashMap::new)
            .insert(key(storage, source), canonical.clone());
    }
    let mut map: HashMap<String, serde_json::Value> =
        std::fs::read_to_string(sidecar_path(storage))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
    map.insert(
        source.to_owned(),
        serde_json::json!({
            "root": canonical.to_string_lossy(),
            "scanned_at": chrono::Utc::now().to_rfc3339(),
        }),
    );
    let _ = std::fs::create_dir_all(storage);
    let _ = std::fs::write(
        sidecar_path(storage),
        serde_json::to_string_pretty(&map).unwrap_or_default(),
    );
}

/// Resolve the root for `source`: in-memory first, then the sidecar.
pub fn resolve(storage: &Path, source: &str) -> Option<PathBuf> {
    {
        let guard = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(root) = guard.as_ref().and_then(|m| m.get(&key(storage, source))) {
            return Some(root.clone());
        }
    }
    let map: HashMap<String, serde_json::Value> = std::fs::read_to_string(sidecar_path(storage))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())?;
    map.get(source)?
        .get("root")?
        .as_str()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Test support: forget one (storage, source) root — not runtime API.
#[cfg(test)]
pub fn roots_for_tests_remove(storage: &Path, source: &str) {
    let mut guard = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = guard.as_mut() {
        map.remove(&key(storage, source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_resolves_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        record(dir.path(), "test-scan", root.path());
        let resolved = resolve(dir.path(), "test-scan").expect("resolved");
        // Canonicalized: /tmp vs /private/tmp style aliases collapse.
        assert!(resolved.ends_with(root.path().file_name().unwrap()));
        assert!(resolve(dir.path(), "unknown-source").is_none());
    }

    #[test]
    fn corrupt_sidecar_degrades_to_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(sidecar_path(dir.path()), "not json").unwrap();
        assert!(resolve(dir.path(), "any").is_none());
    }
}
