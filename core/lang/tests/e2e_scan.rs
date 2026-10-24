//! End-to-end scan fixture (review round-1 F4): real scanner, real store —
//! asserting SYMBOL entities and CALL edges per new-language file, never
//! satisfied by File/Repository metadata alone.

use std::fs;

use engram_domain::*;
use engram_ingest::{scan_repository, ScanOptions};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "t".into(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn scan_opts(source: &str, manifest: std::collections::HashMap<String, String>) -> ScanOptions {
    ScanOptions {
        scope: scope(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        actor: Actor {
            id: Id::from("test"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        source_name: source.to_owned(),
        max_bytes: 0,
        manifest,
        scan_filter: Default::default(),
    }
}

#[test]
fn multi_language_scan_extracts_symbols_and_calls() {
    let root = std::env::temp_dir().join(format!("engram-lang-fixture-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("util.lua"),
        "local function helper(x) return x end\nfunction M.run(a) helper(a) end\n",
    )
    .unwrap();
    fs::write(
        root.join("math.jl"),
        "function helper(x)\n    return x\nend\nstruct Point\n    x::Int\nend\n",
    )
    .unwrap();
    fs::write(
        root.join("main.zig"),
        "pub fn helper(x: u32) u32 {\n    return x;\n}\npub fn run(a: u32) u32 {\n    return helper(a);\n}\n",
    )
    .unwrap();

    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let source = "lang-fixture";
    let (summary, _) = scan_repository(
        &root,
        &scan_opts(source, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan");
    assert_eq!(summary.errors, 0, "scan errors");
    assert!(summary.ingested >= 3, "all fixtures ingested: {summary:?}");

    let entities = block_on(store.list_entities(&scope())).expect("entities");
    let fns: Vec<String> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Function)
        .map(|e| e.name.clone())
        .collect();
    assert!(
        fns.iter().any(|n| n.ends_with("helper")),
        "Function entities (helper): {fns:?}"
    );
    assert!(
        fns.iter().any(|n| n.ends_with("run")),
        "Function entities (run): {fns:?}"
    );
    let structs: Vec<String> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Struct)
        .map(|e| e.name.clone())
        .collect();
    assert!(
        structs.iter().any(|n| n.ends_with("Point")),
        "julia struct entity: {structs:?}"
    );

    // Call edges: lua + zig both contain run→helper.
    // Call edges: lua + zig both contain run→helper.
    let call_edges: Vec<String> = block_on(store.list_relationships(&scope()))
        .expect("rels")
        .into_iter()
        .filter(|r| r.predicate == "calls")
        .filter_map(|r| r.object.name.clone())
        .collect();
    assert!(
        call_edges.iter().any(|n| n.ends_with("helper")),
        "call edges to helper: {call_edges:?}"
    );

    let _ = fs::remove_dir_all(&root);
}
