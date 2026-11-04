//! Wedge regression: two real-world files deadlocked the scan pipeline
//! (0% CPU, no IO, forever) through scan_repository. Isolated 2026-11-04
//! against agentzero: `gateway/src/http/mod.rs` (axum route table) and
//! `runtime/agent-runtime/src/engine/config.rs` (plain config struct).
//! Each fixture must complete a full scan — hang = fail.
//!
//! Run with a timeout so a regression hangs loudly instead of silently:
//!   cargo test --release -p engram-ingest --test poison_files -- --test-threads=1

use std::fs;

use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_store_sqlite::SqlKnowledgeStore;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-1"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scan_fixture(name: &str) {
    let root = std::env::temp_dir().join(format!("engram-poison-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    let content = fs::read_to_string(format!("tests/fixtures/poison/{name}")).expect("fixture");
    fs::write(root.join(name), content).expect("write fixture");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "fixture".to_owned(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    let (summary, _manifest) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    assert_eq!(summary.errors, 0, "no errors: {summary:?}");
    assert!(summary.ingested >= 1, "fixture ingested: {summary:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn route_table_file_scans_to_completion() {
    scan_fixture("route_table.rs");
}

#[test]
fn config_struct_file_scans_to_completion() {
    scan_fixture("config_struct.rs");
}
