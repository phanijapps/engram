//! Live unscored candidate-stage probe. Receives questions, never labels.
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
            .ok_or("usage: code_candidate_probe STORE PROJECT")?,
    )
    .canonicalize()?;
    let project = args.next().ok_or("missing project")?;
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
    let provider = EngramProvider::open(&config)?;
    let scope: Scope = serde_json::from_value(json!({"tenant":"default","workspace":project}))?;
    let requester: Requester =
        serde_json::from_value(json!({"actor":{"id":"code-query-probe","kind":"agent"}}))?;
    let effective = provider
        .code_configuration()
        .ok_or("missing code configuration")?;
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let answer = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let request: engram_domain::CodeEvidenceQuery = serde_json::from_str(&line)?;
            let start = Instant::now();
            let result = block_on(engram_integration::code_candidates::collect(
                &provider,
                &request,
                &scope,
                &requester,
                &effective.effective,
            ))?;
            let parents: Vec<_> = result.candidates.iter().map(|candidate| {
                let code=candidate.parent.code.as_ref().expect("resolved code parent");
                json!({"segmentId":candidate.segment.id,"parentUnitId":candidate.segment.parent_unit_id,
                    "chunkId":candidate.parent.id,"path":code.file.path,"repository":code.file.repository,
                    "anchor":candidate.parent.location.as_ref().and_then(|l|l.anchor.as_ref()),
                    "category":candidate.segment.source_category,"ranges":candidate.segment.ranges,
                    "declarationRange":code.declaration_range,"sourceHash":code.source_hash,
                    "generation":code.generation,"fusionScore":candidate.fusion_score,
                    "fusionTraces":candidate.fusion_traces})
            }).collect();
            Ok(
                json!({"effectiveConfigHash":effective.hash,"candidateCounts":result.candidate_counts,
                "phaseMs":result.phase_ms,"sourceFailures":result.source_failures,
                "queryRepresentationId":result.query_representation_id,"passageRepresentationId":result.representation_id,
                "sourceRevision":result.source_revision,"reindexRequired":result.reindex_required,
                "candidates":parents,"seconds":start.elapsed().as_secs_f64()}),
            )
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
