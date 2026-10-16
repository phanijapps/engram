//! Development live evidence probe; receives queries and a fixed startup calibration, never labels.
use engram_domain::{Requester, Scope, types::ScopeMappingStrategy};
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
    SqliteStorageLayout,
};
use futures::executor::block_on;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let storage = PathBuf::from(
        args.next()
            .ok_or("usage: code_evidence_probe STORE PROJECT [CALIBRATION]")?,
    )
    .canonicalize()?;
    let project = args.next().ok_or("missing project")?;
    let calibration_path = args.next();
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let mut config = EngramConfig::new(
        &storage,
        &storage,
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "fastembed".into(),
            model: "BAAI/bge-small-en-v1.5".into(),
            dimensions: 384,
            prompt_profile: "query".into(),
            normalization: None,
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    );
    config.sqlite_storage_layout = SqliteStorageLayout::SingleFile {
        file_name: "engram_data.db".into(),
    };
    if let Some(path) = calibration_path {
        config = config.with_code_calibration_path(path);
    }
    let provider = EngramProvider::open(&config)?;
    let scope: Scope = serde_json::from_value(json!({"tenant":"default","workspace":project}))?;
    let requester: Requester =
        serde_json::from_value(json!({"actor":{"id":"code-query-probe","kind":"agent"}}))?;
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let answer = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let request: engram_domain::CodeEvidenceQuery = serde_json::from_str(&line)?;
            let binding =
                engram_integration::code_evidence::calibration_template(&provider, &request)?;
            // Independent label-free pre-score observer. The frozen store must
            // remain unchanged; this overhead is not operation/MCP latency.
            let observer_start = Instant::now();
            let observed = block_on(engram_integration::code_candidates::collect(
                &provider,
                &request,
                &scope,
                &requester,
                &provider
                    .code_configuration()
                    .ok_or("missing code config")?
                    .effective,
            ))?;
            let observer_seconds = observer_start.elapsed().as_secs_f64();
            let candidates:Vec<_>=observed.candidates.iter().map(|c| {
                let code=c.parent.code.as_ref().expect("resolved code parent");
                json!({"parentUnitId":c.segment.parent_unit_id,"segmentId":c.segment.id,
                    "path":code.file.path,"anchor":c.parent.location.as_ref().and_then(|l|l.anchor.as_ref()),
                    "category":c.segment.source_category,"declarationRange":code.declaration_range,
                    "sourceHash":code.source_hash,"generation":code.generation,"ranges":c.segment.ranges,
                    "fusionScore":c.fusion_score})
            }).collect();
            let start = Instant::now();
            let packet = block_on(provider.code_evidence(&request, &scope, &requester))?;
            let seconds = start.elapsed().as_secs_f64();
            let revision = block_on(
                provider
                    .code_segments()
                    .ok_or("missing segment authority")?
                    .code_index_revision(),
            )?;
            if revision != observed.source_revision {
                return Err("source/index changed during evaluation observation".into());
            }
            if packet.diagnostics.candidate_counts.get("fused_segments")
                != observed.candidate_counts.get("fused_segments")
            {
                return Err("observer/operation candidate inventories differ".into());
            }
            Ok(json!({"packet":packet,"binding":binding,"seconds":seconds,
                "observerSeconds":observer_seconds,"sourceRevision":revision,"candidates":candidates}))
        })();
        let value = match answer {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        writeln!(stdout, "{value}")?;
        stdout.flush()?;
    }
    Ok(())
}
