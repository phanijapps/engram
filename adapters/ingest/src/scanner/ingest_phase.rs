//! Scan phase 3: the parallel ingest + extraction pass.
//!
//! One rayon closure per queued file: chunk dispatch (tree-sitter candidates
//! where a grammar supports the extension, ingestor-internal chunker
//! otherwise), the single-parse shared-tree extraction (calls + structural +
//! frameworks), cross-file resolution, temporal validity carry-over, graph
//! persistence, and contract-entity extraction.
//!
//! Relocated verbatim from `scan_repository` (scanner god-module split);
//! behavior unchanged — the scanner suite is the regression net.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use engram_domain::*;
use futures::executor::block_on;

use super::contract_phase;
use crate::reconcile;
use crate::scanner::is_markdown_ext;
use crate::{DocumentIngestRequest, DocumentMetadata};
use engram_knowledge::CoreError;
use rayon::prelude::*;

use super::prepass::ReadyToIngest;
use super::{Outcome, ScanOptions};

/// Shared, immutable-per-scan context for the parallel closure. Everything
/// the per-file work needs that is NOT the file itself.
pub(super) struct IngestCtx<'a, R> {
    pub repo: &'a R,
    pub opts: &'a ScanOptions,
    pub source_name: &'a str,
    pub doc_source_kind: SourceKind,
    pub source_key: &'a str,
    pub git_source_metadata: &'a Option<Metadata>,
    pub code_ingestor: &'a crate::KnowledgeIngestor<crate::CodeSymbolChunker>,
    /// Ext-aware fallback chunker (EN-15): unknown extensions never run
    /// foreign-language recognizers; the fallback is lossless.
    pub code_chunker: &'a crate::CodeSymbolChunker,
    pub markdown_ingestor: &'a crate::KnowledgeIngestor<crate::MarkdownChunker>,
    pub text_ingestor: &'a crate::KnowledgeIngestor<crate::PlainTextChunker>,
    pub ts_chunker: Option<&'a crate::TreeSitterChunker>,
    pub extractor: &'a crate::GraphExtractor,
    pub global_entity_names: &'a HashSet<String>,
    /// Repo-wide receiver evidence (plan v6 §4) — fed to call extraction.
    pub repo_ctx: &'a engram_code::parser::repo_context::RepoContext,
    pub name_index: &'a Arc<Mutex<engram_code::SymbolIndex>>,
}

/// Parallel ingest results: per-file outcomes plus chunk-coverage warnings
/// (large files whose chunked bytes cover less than half the file — the
/// fetch-content single-chunk class surfaced at scan time).
pub(super) struct IngestResults {
    pub outcomes: Vec<(String, Outcome, Vec<KnowledgeEntity>)>,
    pub coverage_warnings: Vec<String>,
}

pub(super) fn parallel_ingest<R>(
    ctx: &IngestCtx<'_, R>,
    to_ingest: &[ReadyToIngest],
) -> IngestResults
where
    R: engram_knowledge::KnowledgeRepository
        + engram_knowledge::KnowledgeGraphRepository
        + Send
        + Sync,
{
    let repo = ctx.repo;
    let opts = ctx.opts;
    let source_name = ctx.source_name;
    let doc_source_kind = &ctx.doc_source_kind;
    let source_key = ctx.source_key;
    let git_source_metadata = ctx.git_source_metadata;
    let code_ingestor = ctx.code_ingestor;
    let code_chunker = ctx.code_chunker;
    let markdown_ingestor = ctx.markdown_ingestor;
    let text_ingestor = ctx.text_ingestor;
    let ts_chunker = ctx.ts_chunker;
    let extractor = ctx.extractor;
    let global_entity_names = ctx.global_entity_names;
    let name_index = ctx.name_index;

    let results: Vec<(String, Outcome, Vec<KnowledgeEntity>, Option<String>)> = to_ingest
        .par_iter()
        .map(|item| {
            let rel = &item.rel;
            let kind = item.kind;
            if item.content.len() as u64 > opts.max_bytes() {
                // Defense-in-depth TOCTOU guard (shouldn't fire; pre-pass
                // already checked).
                return (rel.clone(), Outcome::Skipped, Vec::new(), None);
            }
            let text = String::from_utf8_lossy(&item.content).into_owned();
            let text_for_ast = text.clone(); // keep a copy for AST call extraction
            let hash = item.hash.clone();
            let document_kind = match kind {
                crate::classifier::FileKind::Code => SourceDocumentKind::Code,
                crate::classifier::FileKind::Text => SourceDocumentKind::Text,
            };
            let request = DocumentIngestRequest {
                source_kind: doc_source_kind.clone(),
                source_name: source_name.to_owned(),
                scope: opts.scope.clone(),
                document_kind,
                document: DocumentMetadata {
                    path: Some(rel.clone()),
                    ..Default::default()
                },
                text: String::new(), // placeholder — real text used for chunking below
                policy: opts.policy.clone(),
                actor: opts.actor.clone(),
                stable_source_key: Some(source_key.to_owned()),
                repository_key: None,
                git_ref: None,
                source_metadata: git_source_metadata.clone(),
            };
            // Tree-sitter chunking for supported extensions; fallback to the
            // ingestor's internal chunker for others.
            let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            // code-graph-quality [parse-multiplicity]: parse ONCE per file and
            // share the tree across chunking, call extraction, and structural
            // extraction (previously 3 independent parses here + 1 in the
            // pre-pass name collection — 4× the dominant scan CPU).
            let shared_tree = ts_chunker
                .as_ref()
                .filter(|ts| ts.supports(ext))
                .and_then(|ts| ts.parse(&text_for_ast, ext).ok());
            // Chunk-coverage warning (large files chunked under half their
            // bytes) — computed where candidates exist; None on other paths.
            let mut coverage: Option<String> = None;
            let ingested = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                let candidates = match ts.chunk_with_tree(tree, &text, ext) {
                    Ok(c) => c,
                    Err(_) => return (rel.clone(), Outcome::Error, Vec::new(), None),
                };
                coverage = coverage_warning_for(rel, &text, &candidates);
                let mut req = request;
                req.text = text;
                block_on(code_ingestor.ingest_with_candidates(repo, req, candidates))
            } else {
                let mut req = request;
                req.text = text;
                match kind {
                    // EN-15: ext-aware, lossless fallback. Foreign-language
                    // recognizers no longer run on unsupported files (a SQL
                    // `type TEXT,` column stopped discarding the file
                    // prefix), and coverage warnings apply on this path too.
                    crate::classifier::FileKind::Code => {
                        match code_chunker.chunk_for_ext(&req.text, ext) {
                            Ok(candidates) => {
                                coverage = coverage_warning_for(rel, &req.text, &candidates);
                                block_on(code_ingestor.ingest_with_candidates(repo, req, candidates))
                            }
                            Err(_) => {
                                return (rel.clone(), Outcome::Error, Vec::new(), None);
                            }
                        }
                    }
                    crate::classifier::FileKind::Text if is_markdown_ext(ext) => {
                        block_on(markdown_ingestor.ingest(repo, req))
                    }
                    crate::classifier::FileKind::Text => block_on(text_ingestor.ingest(repo, req)),
                }
            };
            let ingested = match ingested {
                Ok(i) => i,
                Err(_) => return (rel.clone(), Outcome::Error, Vec::new(), coverage),
            };
            // AST-level call extraction using the GLOBAL entity name set (not
            // just this file's symbols). This preserves cross-file call edges.
            let ast_calls = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                // Run whenever the grammar supports the file — an empty
                // global name set yields empty calls, NOT a fallback to the
                // fact-less extract_into path (which silently dropped all
                // Phase-2 structural/framework facts for declaration-free
                // files; final-review sweep pin caught it).
                ts.extract_calls_tree(tree, &text_for_ast, ext, global_entity_names, ctx.repo_ctx)
                    .ok()
            } else {
                None
            };
            let frameworks = {
                let mut facts = engram_code::extract_frameworks(&text_for_ast, ext);
                if let (Some(_ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                    // AST-based client API consumption (fetch/axios/useSWR):
                    // attributed to enclosing functions, joined to route
                    // endpoints by the standard ladder.
                    let consumers =
                        engram_code::extract_api_consumers(tree, &text_for_ast, ext);
                    facts.consumes.extend(consumers);
                }
                if facts.is_empty() { None } else { Some(facts) }
            };
            let structural = if let (Some(ts), Some(tree)) = (ts_chunker.as_ref(), shared_tree.as_ref()) {
                ts.extract_structural_tree(tree, &text_for_ast, ext).ok()
            } else {
                None
            };
            let extracted_result = if let Some(ref calls) = ast_calls {
                extractor
                    .extract_with_calls(
                        &ingested.source,
                        &ingested.document,
                        &ingested.chunks,
                        Some(calls),
                        structural.as_ref(),
                        frameworks.as_ref(),
                    )
                    .map(|graph| {
                        // Persist manually (extract_with_calls doesn't persist).
                        let graph2 = graph.clone();
                        (graph, graph2)
                    })
                    .map(|(graph, _)| graph)
            } else {
                core::result::Result::Err(CoreError::InvalidRequest {
                    reason: "no ast calls".to_owned(),
                })
            };
            let extracted = match extracted_result {
                Ok(mut g) => {
                    // C1: cross-file resolution — register entities + resolve refs.
                    if let Ok(mut idx) = name_index.lock() {
                        let repo = ingested
                            .source
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get(crate::source_key::REPOSITORY_KEY))
                            .and_then(|v| v.as_str());
                        let path = ingested.document.path.as_deref();
                        crate::extractor::register_entities(&mut idx, &g.entities, repo, path);
                        g.unresolved =
                            engram_code::resolve_refs(&idx, &mut g.relationships, repo, path, Some(ctx.repo_ctx));
                    }
                    // Temporal carry-over (whats-changed-temporal-flat): a
                    // re-ingested file gets a brand-new graph id, so every
                    // entity id churns even when the symbol is unchanged.
                    // Inherit the prior graph's validFrom for symbols whose
                    // (kind, name) identity survives — `recent` then moves
                    // only for genuinely new/renamed symbols, not for the
                    // 95% unchanged siblings of a changed file.
                    for entity in &mut g.entities {
                        let key = reconcile::validity_key(
                            &format!("{:?}", entity.kind),
                            &entity.name,
                        );
                        if let Some(prior) = item.prior_validity.get(&key) {
                            entity.valid_from = Some(*prior);
                        }
                    }
                    // Persist the graph + entities + relationships + ledger.
                    let _ = block_on(async {
                        repo.put_graph(g.graph.clone()).await?;
                        for entity in &g.entities {
                            repo.put_entity(entity.clone()).await?;
                        }
                        for rel in &g.relationships {
                            repo.put_relationship(rel.clone()).await?;
                        }
                        if !g.unresolved.is_empty()
                            && let Err(e) = repo.put_unresolved_refs(g.unresolved.clone()).await {
                                // Sibling pattern of the five warning-tagged
                                // scans below: surface, never swallow.
                                eprintln!(
                                    "[engram-ingest] warning: failed to persist ledger rows for '{rel}': {e}"
                                );
                            }
                        for (chunk_idx, entity_refs) in &g.chunk_entities {
                            if let Some(chunk) = ingested.chunks.get(*chunk_idx) {
                                let mut updated = chunk.clone();
                                updated.entities = entity_refs.clone();
                                repo.put_chunk(updated).await?;
                            }
                        }
                        Ok::<(), CoreError>(())
                    });
                    g
                }
                Err(_) => {
                    let mut idx = name_index.lock().unwrap();
                    match block_on(extractor.extract_into(
                        repo,
                        &ingested.source,
                        &ingested.document,
                        &ingested.chunks,
                        Some(&mut *idx),
                    )) {
                        Ok(e) => e,
                        Err(_) => return (rel.clone(), Outcome::Error, Vec::new(), coverage),
                    }
                }
            };
            // Contract extraction (T4/T5/T6): for YAML/JSON files, attempt
            // OpenAPI detection and emit EntityKind::Api entities + exposes edges.
            // Uses `text_for_ast` (a pre-move clone of the file text) so we do
            // not need to read the content a second time.
            let (contract_keys, contract_parse_failed, contract_had_write_error) =
                contract_phase::extract_contract_entities(
                    repo,
                    &opts.scope,
                    source_key,
                    &text_for_ast,
                    ext,
                    &ingested.source.provenance,
                );

            let emitted_entities = extracted.entities.clone();
            (
                rel.clone(),
                Outcome::Ingested {
                    entities: extracted.entities.len(),
                    relationships: extracted.relationships.len(),
                    hash: (rel.clone(), hash),
                    contract_keys,
                    contract_parse_failed,
                    contract_had_write_error,
                },
                emitted_entities,
                coverage,
            )
        })
        .collect();

    // Split outcomes from warnings; cap the warning list so a pathological
    // scan cannot flood the summary.
    let mut coverage_warnings: Vec<String> = Vec::new();
    let outcomes = results
        .into_iter()
        .map(|(rel, outcome, entities, warning)| {
            if let Some(warning) = warning
                && coverage_warnings.len() < super::MAX_COVERAGE_WARNINGS
            {
                coverage_warnings.push(warning);
            }
            (rel, outcome, entities)
        })
        .collect();
    IngestResults {
        outcomes,
        coverage_warnings,
    }
}

/// Chunk-coverage warning for one file, when warranted: large files whose
/// chunked bytes cover less than [`super::COVERAGE_MIN_RATIO`] of the file.
/// Small files chunk sparsely by design and are exempt.
fn coverage_warning_for(
    rel: &str,
    text: &str,
    candidates: &[engram_code::ChunkCandidate],
) -> Option<String> {
    let file_bytes = text.len();
    if file_bytes < super::COVERAGE_MIN_FILE_BYTES {
        return None;
    }
    let chunked: usize = candidates.iter().map(|c| c.text.len()).sum();
    let ratio = chunked as f64 / file_bytes as f64;
    (ratio < super::COVERAGE_MIN_RATIO).then(|| {
        format!(
            "{rel}: {:.0}% chunk coverage ({chunked} of {file_bytes} bytes chunked)",
            ratio * 100.0
        )
    })
}
