//! engram-code scale benchmark: scan a repository with the full Phase-2
//! pipeline (AST chunking + typed edges + resolution + ledger + orphan
//! sweep), then a one-file-edit re-scan, and print timing + counts.
//! Usage: `cargo run -p engram-ingest --example benchmark_code_index -- <repo-path>`.

use std::path::PathBuf;
use std::time::Instant;

use engram_domain::*;
use engram_ingest::{ScanFilter, ScanOptions, scan_repository};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "bench".to_owned(),
        subject: None,
        workspace: Some("code-index".to_owned()),
        session: None,
        environment: None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: benchmark_code_index <repo-path>");
    let store = SqlKnowledgeStore::open_in_memory()?;
    let opts = ScanOptions {
        scope: scope(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Medium),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        },
        actor: Actor {
            id: Id::from("bench"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        source_name: "bench-repo".to_owned(),
        max_bytes: 4 * 1024 * 1024,
        manifest: Default::default(),
        scan_filter: ScanFilter::default(),
    };

    let started = Instant::now();
    let (summary, manifest) = scan_repository(&root, &opts, &store, |_| {})?;
    let full = started.elapsed();

    let bench_scope = scope();
    let entities = block_on(store.list_entities(&bench_scope))?.len();
    let rels = block_on(store.list_relationships(&bench_scope))?;
    let by_kind = |k: &str| rels.iter().filter(|r| r.predicate == k).count();
    let pending =
        block_on(store.list_unresolved_refs(&scope(), UnresolvedReferenceStatus::Pending))?.len();

    // One-file-edit sync: touch a small source file, re-scan with the manifest.
    let mut edit_path = None;
    let mut walker = |dir: &std::path::Path| -> Option<std::path::PathBuf> {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).ok()? {
                let entry = entry.ok()?;
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "rs" || e == "ts") {
                    return Some(p);
                }
            }
        }
        None
    };
    if let Some(p) = walker(&root) {
        let text = std::fs::read_to_string(&p)?;
        std::fs::write(&p, format!("{text}\nfn bench_touch_marker() {{}}\n"))?;
        edit_path = Some(p);
    }
    let sync_started = Instant::now();
    let opts2 = ScanOptions { manifest, ..opts };
    let (summary2, _) = scan_repository(&root, &opts2, &store, |_| {})?;
    let sync = sync_started.elapsed();

    println!("engram-code Phase-2 index benchmark");
    println!("  repo: {}", root.display());
    println!("  files ingested: {}", summary.ingested);
    println!("  full index: {full:?}");
    println!("  entities: {entities}");
    println!(
        "  relationships: {} (calls={}, imports={}, contains={}, extends={}, implements={}, routes_to={})",
        rels.len(),
        by_kind("calls"),
        by_kind("imports"),
        by_kind("contains"),
        by_kind("extends"),
        by_kind("implements"),
        by_kind("routes_to")
    );
    println!("  pending ledger rows: {pending}");
    println!(
        "  one-file-edit sync: {sync:?} (re-ingested {})",
        summary2.ingested
    );
    if let Some(p) = edit_path {
        // restore the file
        let text = std::fs::read_to_string(&p)?;
        if let Some(stripped) = text.strip_suffix("\nfn bench_touch_marker() {}\n") {
            std::fs::write(&p, stripped)?;
        }
    }
    Ok(())
}
