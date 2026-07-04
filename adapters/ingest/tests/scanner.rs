use std::fs;

use engram_domain::*;
use engram_ingest::{STABLE_SOURCE_KEY, ScanOptions, scan_repository};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

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

// ---------------------------------------------------------------------------
// Structured-repo-identity: scanner wiring (AC-1 / AC-6)
// ---------------------------------------------------------------------------

/// Runs a git command inside `root`. Panics on failure.
fn git(root: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git command");
    if !out.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Tests AC-1: a git-backed scan sets `SourceKind::GitRepository` and derives
/// the stable-source-key from the normalized remote.
#[test]
fn scanner_git_repo_sets_git_repository_kind_and_stable_key() {
    let root = std::env::temp_dir().join(format!(
        "engram-scan-git-{}-{}",
        std::process::id(),
        "wiring"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    // Minimal git repo with a remote — enough for detect_git to succeed.
    git(&root, &["init"]);
    git(&root, &["config", "user.email", "ci@test.local"]);
    git(&root, &["config", "user.name", "CI"]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/scan-test.git",
        ],
    );
    fs::write(root.join("README.md"), "# scan test\n").expect("write README");
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-m", "init"]);

    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "scan-test".to_owned(),
        max_bytes: 0,
        manifest: Default::default(),
    };
    let (summary, _) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");

    // Git was detected.
    assert!(
        summary.git_remote.is_some(),
        "git_remote must be set for a git-backed dir"
    );

    // The persisted source is tagged GitRepository and carries the normalized key.
    let sources = block_on(store.list_sources(&scope())).expect("list_sources");
    assert_eq!(sources.len(), 1, "one source per scan");
    assert_eq!(
        sources[0].kind,
        SourceKind::GitRepository,
        "source_kind must be GitRepository for a git-backed scan"
    );
    let key = sources[0]
        .metadata
        .as_ref()
        .and_then(|m| m.get(STABLE_SOURCE_KEY))
        .and_then(|v| v.as_str());
    assert_eq!(
        key,
        Some("github.com/acme/scan-test"),
        "stable_source_key must be the normalized remote (scheme/.git stripped)"
    );

    // Query-by-key works end-to-end (T6 AC-4).
    let graphs = block_on(store.list_graphs_by_source(&scope(), "github.com/acme/scan-test"))
        .expect("list_graphs_by_source");
    assert!(!graphs.is_empty(), "at least one graph from the git repo");

    let _ = fs::remove_dir_all(&root);
}

/// Tests AC-6: a non-git directory falls back to the un-enriched source name as
/// the stable-source-key and is tagged `Filesystem`.
#[test]
fn scanner_non_git_uses_fallback_key_and_filesystem_kind() {
    let root = std::env::temp_dir().join(format!(
        "engram-scan-nongit-{}-{}",
        std::process::id(),
        "wiring"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("notes.md"), "# notes\nsome text\n").expect("write notes");

    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "local-notes".to_owned(),
        max_bytes: 0,
        manifest: Default::default(),
    };
    let (summary, _) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");

    assert!(
        summary.git_remote.is_none(),
        "no git_remote for a plain directory"
    );
    assert_eq!(summary.ingested, 1, "notes.md should be ingested");

    let sources = block_on(store.list_sources(&scope())).expect("list_sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].kind,
        SourceKind::Filesystem,
        "non-git source must be tagged Filesystem"
    );
    // Fallback key = normalize_fallback(source_name) = source_name.to_lowercase()
    let key = sources[0]
        .metadata
        .as_ref()
        .and_then(|m| m.get(STABLE_SOURCE_KEY))
        .and_then(|v| v.as_str());
    assert_eq!(
        key,
        Some("local-notes"),
        "fallback stable_source_key must equal the normalized source name"
    );

    let _ = fs::remove_dir_all(&root);
}
