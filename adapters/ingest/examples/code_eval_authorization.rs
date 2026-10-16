//! Seed isolated, source-owned denial controls for the real-MCP evaluator.
use engram_domain::*;
use engram_ingest::{ScanFilter, ScanOptions, scan_repository};
use engram_knowledge::CodeSourceRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("fixture root required")?);
    std::fs::create_dir_all(root.join("store"))?;
    let store = SqlKnowledgeStore::open_file(root.join("store/engram_data.db"))?;
    let scope: Scope =
        serde_json::from_value(json!({"tenant":"default","workspace":"code-eval-denial"}))?;
    let mut reports = Vec::new();
    for (name, policy) in [
        (
            "private",
            json!({"visibility":"private","retention":"durable","allowedUses":["retrieval"]}),
        ),
        (
            "expired",
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"],"expiresAt":"2000-01-01T00:00:00Z"}),
        ),
    ] {
        let corpus = root.join(name);
        std::fs::create_dir_all(&corpus)?;
        let text = format!(
            "export function {name}LaunchSequence() {{\n  // Decode the orbital launch confirmation phrase.\n  return '{name}-orbital-confirmation';\n}}\n"
        );
        // Dedicated fixture output only: never replace an operator's source.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(corpus.join("launch.ts"))?;
        file.write_all(text.as_bytes())?;
        let opts = ScanOptions {
            scope: scope.clone(),
            actor: serde_json::from_value(json!({"id":"eval-private-owner","kind":"agent"}))?,
            policy: serde_json::from_value(policy)?,
            source_name: format!("code-eval-{name}"),
            max_bytes: 0,
            manifest: Default::default(),
            scan_filter: ScanFilter::default(),
        };
        let (summary, _) = scan_repository(&corpus, &opts, &store, |_| {})?;
        if summary.errors != 0 {
            return Err("authorization fixture scan failed".into());
        }
        let m = block_on(store.list_code_sources(&scope))?
            .into_iter()
            .find(|m| m.file.repository == opts.source_name)
            .ok_or("fixture source missing")?;
        let owner_read = block_on(store.read_code_source(
            CodeSourceReadRequest {
                file: m.file.clone(),
                requester: serde_json::from_value(
                    json!({"actor":{"id":"eval-private-owner","kind":"agent"}}),
                )?,
                generation: Some(m.generation.clone()),
                max_bytes: 4096,
            },
            &scope,
        ));
        if name == "private" && !owner_read.as_ref().is_ok_and(|r| r.text == text) {
            return Err("authorized owner positive control failed".into());
        }
        if name == "expired"
            && !matches!(
                owner_read,
                Err(engram_knowledge::CoreError::PolicyDenied { .. })
            )
        {
            return Err("expired owner negative control failed".into());
        }
        reports.push(
            json!({"kind":name,"file":m.file,"generation":m.generation,"sourceHash":m.source_hash,
            "canonicalPresent":true,"ownerReadAllowed":name=="private"}),
        );
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(())
}
