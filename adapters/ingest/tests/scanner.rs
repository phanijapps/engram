use std::fs;

use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;

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

#[test]
fn scans_fixture_skipping_secrets_oversized_and_denylist() {
    let root =
        std::env::temp_dir().join(format!("engram-scan-{}-{}", std::process::id(), "fixture"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("node_modules")).expect("create node_modules");
    fs::write(
        root.join("main.rs"),
        "fn alpha() { beta(); }\nfn beta() {}\n",
    )
    .expect("write main.rs");
    fs::write(
        root.join("README.md"),
        "# demo\nengram keeps memory and knowledge separate.\n",
    )
    .expect("write README");
    // Secret carrier (non-hidden so the walker reaches it; the scanner's secret
    // blocklist must skip it without reading).
    fs::write(root.join("id_rsa"), "TOPSECRET\n").expect("write id_rsa");
    // Oversized text file.
    fs::write(root.join("big.txt"), "x".repeat(2000)).expect("write big.txt");
    // Denied directory.
    fs::write(root.join("node_modules/x.js"), "console.log(1)\n").expect("write nm");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "fixture".to_owned(),
        max_bytes: 1024,
        manifest: Default::default(),
    };
    let (summary, manifest) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");

    assert_eq!(summary.ingested, 2, "main.rs + README.md: {summary:?}");
    assert!(summary.entities >= 2, "entities: {summary:?}");
    // id_rsa (secret) + big.txt (oversized) + node_modules/x.js (denylist) skipped.
    assert!(summary.skipped >= 3, "skipped: {summary:?}");

    // Incremental: re-scan with the manifest → prior files unchanged, none re-ingested.
    let opts2 = ScanOptions {
        manifest,
        ..opts.clone()
    };
    let (summary2, _manifest2) = scan_repository(&root, &opts2, &store, |_| {}).expect("rescan");
    assert_eq!(summary2.ingested, 0, "nothing re-ingested: {summary2:?}");
    assert_eq!(
        summary2.unchanged, summary.ingested,
        "unchanged: {summary2:?}"
    );

    let _ = fs::remove_dir_all(&root);
}
