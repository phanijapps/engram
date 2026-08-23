use std::fs;

use engram_domain::*;
use engram_ingest::{STABLE_SOURCE_KEY, ScanOptions, scan_repository};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_sqlite::SqlKnowledgeStore;
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
        scan_filter: engram_ingest::ScanFilter::default(),
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
        scan_filter: engram_ingest::ScanFilter::default(),
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
        scan_filter: engram_ingest::ScanFilter::default(),
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

#[test]
fn scan_honors_custom_deny_filter() {
    // A custom denylist (built from a ScanFilterConfig) skips a directory the
    // builtin would index, while the builtin denylist still applies.
    let root = std::env::temp_dir().join(format!(
        "engram-scan-{}-{}",
        std::process::id(),
        "custom-deny"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("generated")).expect("create generated");
    fs::write(
        root.join("main.rs"),
        "fn alpha() { beta(); }\nfn beta() {}\n",
    )
    .expect("write main.rs");
    // `generated/` is NOT in the builtin denylist → would be indexed without a
    // custom filter.
    fs::write(root.join("generated/gen.rs"), "fn gen() {}\n").expect("write generated/gen.rs");

    let cfg = engram_ingest::ScanFilterConfig::from_json(
        r#"{ "deny": { "dirs": ["generated"], "extensions": ["map"] } }"#,
    )
    .expect("parse config");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    // With the custom filter, generated/gen.rs is skipped → only main.rs ingested.
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "fixture".to_owned(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::merge(&cfg),
    };
    let (summary, _manifest) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    assert_eq!(summary.ingested, 1, "only main.rs: {summary:?}");

    // With the builtin filter (no custom deny), generated/gen.rs IS ingested.
    let opts_builtin = ScanOptions {
        scan_filter: engram_ingest::ScanFilter::default(),
        ..opts.clone()
    };
    let (summary_b, _manifest_b) =
        scan_repository(&root, &opts_builtin, &store, |_| {}).expect("scan builtin");
    assert_eq!(
        summary_b.ingested, 2,
        "main.rs + generated/gen.rs: {summary_b:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// RFC-0020 Phase 2 typed structural edges: scans yield `imports`
/// (file→module), `contains` (receiver-qualified containment), `extends` /
/// `implements` (inheritance) relationships beside `calls`. One code file per
/// scan: a known store-side upsert bug (NULL `archived_at` read on the shared
/// repository entity, pre-existing — see notes.md) aborts the second file's
/// persistence in multi-file scans.
#[test]
fn scan_yields_typed_structural_edges_typescript() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-struct-ts", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(
        root.join("repo.ts"),
        "import { helper } from './utils';\nclass Repo extends Base implements Store {\n    find(): void {}\n}\n",
    )
    .expect("write repo.ts");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "structural-fixture".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    let rels = block_on(store.list_relationships(&scope())).expect("list rels");
    let has = |predicate: &str, subject: &str, object: &str| {
        rels.iter().any(|r| {
            r.predicate == predicate
                && r.subject.name.as_deref() == Some(subject)
                && r.object.name.as_deref() == Some(object)
        })
    };
    assert!(has("imports", "repo.ts", "./utils"), "imports: {rels:?}");
    assert!(has("contains", "Repo", "Repo::find"), "contains: {rels:?}");
    assert!(has("extends", "Repo", "Base"), "extends: {rels:?}");
    assert!(has("implements", "Repo", "Store"), "implements: {rels:?}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn scan_yields_typed_structural_edges_rust() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-struct-rs", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(
        root.join("main.rs"),
        "use std::fmt;\nstruct Cache;\nimpl Store for Cache {\n    fn get(&self) -> u8 { 0 }\n}\ntrait Store {}\n",
    )
    .expect("write main.rs");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "structural-fixture-rs".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    let rels = block_on(store.list_relationships(&scope())).expect("list rels");
    let has = |predicate: &str, subject: &str, object: &str| {
        rels.iter().any(|r| {
            r.predicate == predicate
                && r.subject.name.as_deref() == Some(subject)
                && r.object.name.as_deref() == Some(object)
        })
    };
    assert!(has("imports", "main.rs", "std::fmt"), "imports: {rels:?}");
    assert!(has("contains", "Cache", "Cache::get"), "contains: {rels:?}");
    assert!(has("implements", "Cache", "Store"), "implements: {rels:?}");
    let _ = fs::remove_dir_all(&root);
}

/// RFC-0020 Phase 2 orphan sweep: a reference to a symbol nothing defines is
/// ledgered as pending; when a LATER scan ingests the defining file, the
/// sweep heals the row (→ resolved) and writes the calls edge without
/// re-ingesting the referring file.
#[test]
fn orphan_sweep_heals_pending_references_on_later_scans() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-sweep", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(
        root.join("caller.rs"),
        "fn orchestrate() {\n    ghost_fn();\n}\n",
    )
    .expect("write caller.rs");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "sweep-fixture".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    let (_summary, manifest1) = scan_repository(&root, &opts, &store, |_| {}).expect("scan 1");

    let pending = block_on(
        store.list_unresolved_refs(&scope(), engram_domain::UnresolvedReferenceStatus::Pending),
    )
    .expect("list pending");
    assert_eq!(
        pending.len(),
        1,
        "ghost reference must be ledgered: {pending:?}"
    );
    assert_eq!(pending[0].reference_name, "ghost_fn");

    // Later scan: the defining file lands; caller.rs is unchanged (manifest
    // carries its hash so it is skipped — no re-ingest).
    fs::write(root.join("defs.rs"), "fn ghost_fn() {}\n").expect("write defs.rs");
    let opts2 = ScanOptions {
        manifest: manifest1,
        ..opts
    };
    scan_repository(&root, &opts2, &store, |_| {}).expect("scan 2");

    let still_pending = block_on(
        store.list_unresolved_refs(&scope(), engram_domain::UnresolvedReferenceStatus::Pending),
    )
    .expect("list pending 2");
    assert!(
        still_pending.is_empty(),
        "sweep must heal the row: {still_pending:?}"
    );
    let resolved = block_on(
        store.list_unresolved_refs(&scope(), engram_domain::UnresolvedReferenceStatus::Resolved),
    )
    .expect("list resolved");
    assert_eq!(
        resolved.len(),
        1,
        "healed row flips to resolved: {resolved:?}"
    );

    let rels = block_on(store.list_relationships(&scope())).expect("rels");
    let ents = block_on(store.list_entities(&scope())).expect("entities");
    let orchestrate_id = ents
        .iter()
        .find(|e| e.name == "orchestrate")
        .expect("orchestrate entity")
        .id
        .clone();
    assert!(
        rels.iter().any(|r| r.predicate == "calls"
            && r.subject.id.as_ref() == Some(&orchestrate_id)
            && r.object.name.as_deref() == Some("ghost_fn")
            && r.object.id.is_some()),
        "sweep must write the healed calls edge: {rels:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Re-ingesting a file retracts its prior ledger rows (regenerated by the
/// fresh extraction pass).
#[test]
fn reingest_retracts_prior_ledger_rows() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-ledretract", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(
        root.join("caller.rs"),
        "fn orchestrate() {\n    ghost_fn();\n}\n",
    )
    .expect("write caller.rs");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "ledretract-fixture".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    scan_repository(&root, &opts, &store, |_| {}).expect("scan 1");
    let pending = block_on(
        store.list_unresolved_refs(&scope(), engram_domain::UnresolvedReferenceStatus::Pending),
    )
    .expect("list pending");
    assert_eq!(pending.len(), 1);

    // Change the referring file — re-ingest retracts the old graph (and its
    // ledger rows) and regenerates extraction for the new content.
    fs::write(
        root.join("caller.rs"),
        "fn orchestrate() {\n    other_ghost();\n}\n",
    )
    .expect("rewrite caller.rs");
    scan_repository(&root, &opts, &store, |_| {}).expect("scan 2");
    let pending2 = block_on(
        store.list_unresolved_refs(&scope(), engram_domain::UnresolvedReferenceStatus::Pending),
    )
    .expect("list pending 2");
    assert!(
        pending2.iter().all(|r| r.reference_name != "ghost_fn"),
        "the old row must be retracted with its graph: {pending2:?}"
    );
    assert!(
        pending2.iter().any(|r| r.reference_name == "other_ghost"),
        "the new extraction regenerates its own row: {pending2:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// RFC-0020 Phase 2 framework resolvers: an Express route yields an
/// `Endpoint` entity wired to its handler via `routes_to`; a React
/// `onClick={handler}` prop yields a `calls` edge from the component.
#[test]
fn scan_extracts_framework_routes_and_callbacks() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-frameworks", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");
    fs::write(
        root.join("server.ts"),
        "const app = {} as any;\nfunction listUsers() {}\nfunction createUser() {}\napp.get('/users', listUsers);\napp.post('/users', createUser);\n",
    )
    .expect("write server.ts");
    fs::write(
        root.join("button.tsx"),
        "function handleClick() {}\nfunction UserButton() {\n  return <button onClick={handleClick}>go</button>;\n}\n",
    )
    .expect("write button.tsx");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "frameworks-fixture".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    let (summary, _) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    assert_eq!(summary.ingested, 2, "both files: {summary:?}");

    let rels = block_on(store.list_relationships(&scope())).expect("rels");
    assert!(
        rels.iter().any(|r| r.predicate == "routes_to"
            && r.subject.name.as_deref() == Some("GET /users")
            && r.object.name.as_deref() == Some("listUsers")),
        "express route edge missing: {rels:?}"
    );
    assert!(
        rels.iter().any(|r| r.predicate == "routes_to"
            && r.subject.name.as_deref() == Some("POST /users")
            && r.object.name.as_deref() == Some("createUser")),
        "second route edge missing: {rels:?}"
    );
    let ents = block_on(store.list_entities(&scope())).expect("entities");
    assert!(
        ents.iter().any(|e| e.name == "GET /users"),
        "endpoint entity missing: {ents:?}"
    );
    // React callback: UserButton --calls--> handleClick (T7/AC6).
    assert!(
        rels.iter().any(|r| r.predicate == "calls"
            && r.subject.name.as_deref() == Some("UserButton")
            && r.object.name.as_deref() == Some("handleClick")),
        "react callback edge missing: {rels:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// RFC-0020 Phase 2 end-to-end (T8): a multi-file polyglot scan produces
/// receiver-qualified identities, all six edge kinds, cross-file resolution
/// with the ledger, and converges on re-scan.
#[test]
fn polyglot_scan_end_to_end_phase2() {
    let root = std::env::temp_dir().join(format!("engram-scan-{}-polyglot", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("create src");
    // Rust: impl receiver + cross-file call + a use import.
    fs::write(
        root.join("src/engine.rs"),
        "use crate::store;\nstruct Engine;\nimpl Engine {\n    fn drive(&self) {\n        store::save();\n        helper();\n    }\n}\n",
    )
    .expect("write engine.rs");
    // Rust: same-named symbol in a second file (bare-name collision →
    // same-doc disambiguation or ledgered ambiguity) + the helper + save.
    fs::write(
        root.join("src/store.rs"),
        "mod inner { pub fn helper() {} }\nstruct Engine;\nimpl Engine {\n    fn idle(&self) {}\n}\npub fn save() {}\n",
    )
    .expect("write store.rs");
    // TS: extends/implements/contains/imports + an Express route.
    fs::write(
        root.join("src/api.ts"),
        "import { helper } from './store';\nclass Repo extends Base implements Store {\n    find(): void {}\n}\nconst app = {} as any;\napp.get('/items', helper);\n",
    )
    .expect("write api.ts");

    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let opts = ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "polyglot".to_owned(),
        max_bytes: 1024 * 1024,
        manifest: Default::default(),
        scan_filter: engram_ingest::ScanFilter::default(),
    };
    let (summary, manifest) = scan_repository(&root, &opts, &store, |_| {}).expect("scan");
    assert_eq!(summary.ingested, 3, "all three files: {summary:?}");

    let rels = block_on(store.list_relationships(&scope())).expect("rels");
    let has = |predicate: &str, subject: &str, object: &str| {
        rels.iter().any(|r| {
            r.predicate == predicate
                && r.subject.name.as_deref() == Some(subject)
                && r.object.name.as_deref() == Some(object)
        })
    };
    // All six edge kinds.
    assert!(has("calls", "Engine::drive", "save"), "calls: {rels:?}");
    assert!(
        has("contains", "Engine", "Engine::drive"),
        "contains: {rels:?}"
    );
    assert!(
        has("contains", "inner", "inner::helper"),
        "mod contains: {rels:?}"
    );
    assert!(has("extends", "Repo", "Base"), "extends: {rels:?}");
    assert!(has("implements", "Repo", "Store"), "implements: {rels:?}");
    assert!(
        has("routes_to", "GET /items", "helper"),
        "routes_to: {rels:?}"
    );
    assert!(
        rels.iter()
            .any(|r| r.predicate == "imports" && r.subject.name.as_deref() == Some("src/api.ts")),
        "imports: {rels:?}"
    );
    // Receiver-qualified identities coexist for the same bare name across
    // files (Engine in engine.rs and store.rs are distinct entities).
    let ents = block_on(store.list_entities(&scope())).expect("entities");
    let engines: Vec<_> = ents.iter().filter(|e| e.name == "Engine").collect();
    assert!(
        engines.len() >= 2
            && engines
                .iter()
                .map(|e| e.id.to_string())
                .collect::<std::collections::HashSet<_>>()
                .len()
                == engines.len(),
        "same-named Engine entities must be distinct: {ents:?}"
    );
    // Convergence: an identical re-scan changes nothing (manifest skip).
    let (summary2, _) =
        scan_repository(&root, &opts2_with(manifest), &store, |_| {}).expect("rescan");
    assert_eq!(
        summary2.ingested, 0,
        "unchanged files are skipped: {summary2:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

fn opts2_with(manifest: std::collections::HashMap<String, String>) -> ScanOptions {
    ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: "polyglot".to_owned(),
        max_bytes: 1024 * 1024,
        manifest,
        scan_filter: engram_ingest::ScanFilter::default(),
    }
}
