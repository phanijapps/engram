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
            .ok_or("usage: code_candidate_probe STORE PROJECT")?,
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
    let mut provider = EngramProvider::open(&config)?;
    if let Some(path) = calibration_path {
        use std::io::Read;
        let file = std::fs::File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err("calibration must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err("calibration exceeds 64 KiB".into());
        }
        provider = provider.with_code_calibration(serde_json::from_slice(&bytes)?);
    }
    let scope: Scope = serde_json::from_value(json!({"tenant":"default","workspace":project}))?;
    let requester: Requester =
        serde_json::from_value(json!({"actor":{"id":"code-query-probe","kind":"agent"}}))?;
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let answer = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let request: engram_domain::CodeEvidenceQuery = serde_json::from_str(&line)?;
            let start = Instant::now();
            let packet = block_on(provider.code_evidence(&request, &scope, &requester))?;
            let binding =
                engram_integration::code_evidence::calibration_template(&provider, &request)?;
            Ok(json!({"packet":packet,"binding":binding,"seconds":start.elapsed().as_secs_f64()}))
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
