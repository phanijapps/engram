//! Development-only vector query probe. Receives questions/instructions, never
//! relevance labels. Returns source identities for separate offline judging.
use engram_domain::{CodeSourceCategory, Requester, Scope, types::ScopeMappingStrategy};
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
    SqliteStorageLayout, code_query::prepare_code_query, code_representation::code_representation,
};
use engram_retrieval::CodeQueryInstruction;
use futures::executor::block_on;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Instant,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    query: String,
    instruction: CodeQueryInstruction,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let storage = PathBuf::from(args.next().ok_or("usage: code_query_probe STORE PROJECT")?)
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
    let model = provider.require_embedding_provider()?;
    let effective = provider
        .code_configuration()
        .ok_or("missing code configuration")?;
    let representation = code_representation(model.as_ref(), &effective.effective.index)?;
    let vectors = provider.code_vectors().ok_or("missing code vector index")?;
    if vectors.embedding_space() != &representation.embedding_space {
        return Err("code representation mismatch".into());
    }
    let segments = provider
        .code_segments()
        .ok_or("missing code segment repository")?;
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let answer = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let request: Request = serde_json::from_str(&line)?;
            let start = Instant::now();
            let query = prepare_code_query(model.as_ref(), &request.query, request.instruction)?;
            let vector = model.embed_prepared_code_query(&query)?;
            let hits = block_on(vectors.search(vectors.embedding_space(), vector, 50))?;
            let raw_count = hits.len();
            let mut seen = HashSet::new();
            let mut parents = Vec::new();
            for (id, distance) in hits {
                let Some(resolved) = block_on(segments.resolve_code_segment(
                    id.as_str(),
                    &scope,
                    &requester,
                    &representation.id,
                ))?
                else {
                    continue;
                };
                if resolved.segment.source_category != CodeSourceCategory::Implementation
                    || !seen.insert(resolved.parent.id.clone())
                {
                    continue;
                }
                let parent = resolved.parent;
                let code = parent.code.ok_or("missing canonical coverage")?;
                parents.push(json!({"parentUnitId":code.unit,"chunkId":parent.id,"path":code.file.path,"repository":code.file.repository,"anchor":parent.location.and_then(|l| l.anchor),"declarationRange":code.declaration_range,"ranges":code.ranges,"sourceHash":code.source_hash,"generation":code.generation,"distance":distance,"segmentId":id}));
            }
            Ok(
                json!({"effectiveConfigHash":effective.hash,"effectiveIndexConfig":effective.effective.index,"embeddingSpace":vectors.embedding_space(),"queryRepresentationId":query.representation_id,"passageRepresentationId":representation.id,"queryTokens":query.coverage,"tokenizerId":query.tokenizer_id,"rawSegmentHits":raw_count,"parents":parents,"seconds":start.elapsed().as_secs_f64()}),
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
