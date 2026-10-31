//! Prepare and persist attempt-owned code records without reconciling the published generation.
use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, ExtractedGraph, FileKind,
    GraphExtractor, IngestedKnowledge, KnowledgeIngestor, ScanOptions, classify_file, content_hash,
    is_secret_file, register_entities, resolve_call_refs,
};
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use std::collections::{BTreeMap, HashSet};

pub struct CommittedGenerationFiles<'a> {
    pub commit: &'a crate::git_source::ResolvedCommit,
    pub files: &'a [crate::git_source::CommittedFile],
}

#[derive(Debug)]
pub struct StagedDocument {
    pub records: IngestedKnowledge,
    pub graph: ExtractedGraph,
    pub source: IndexedSource,
}
#[derive(Debug)]
pub struct PreparedGeneration {
    pub binding: CodeBinding,
    pub generation: CodeGeneration,
    pub documents: Vec<StagedDocument>,
    /// Target IDs and complete contributor inventories, including generation-owned repository anchors.
    pub lineage: BTreeMap<String, CodeLineage>,
}
fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
fn metadata(binding: &CodeBinding, generation: &CodeGeneration) -> Metadata {
    let mut meta = Metadata::default();
    meta.insert("codeRepoId".into(), binding.repo_id.0.clone().into());
    meta.insert("codeGenerationId".into(), generation.id.clone().into());
    meta.insert(
        "codeBindingVersion".into(),
        generation.binding_version.into(),
    );
    meta.insert("repository".into(), binding.repo_id.0.clone().into());
    meta.insert("branch".into(), binding.branch_ref.clone().into());
    meta.insert("revision".into(), generation.commit_oid.clone().into());
    meta
}
fn stamp(target: &mut Option<Metadata>, stamp: &Metadata) {
    target
        .get_or_insert_with(Metadata::default)
        .extend(stamp.clone());
}

/// Prepare only committed code files; ordinary document ingestion remains a separate operation.
pub fn prepare_generation(
    binding: &CodeBinding,
    generation: &CodeGeneration,
    committed: &CommittedGenerationFiles<'_>,
    options: &ScanOptions,
) -> CoreResult<PreparedGeneration> {
    binding
        .validate()
        .map_err(|_| invalid("invalid code binding"))?;
    generation
        .validate()
        .map_err(|_| invalid("invalid code generation"))?;
    if options.scope != binding.scope
        || generation.binding_version != binding.version
        || committed.commit.branch_ref != binding.branch_ref
        || committed.commit.oid != generation.commit_oid
    {
        return Err(invalid(
            "generation does not match the authorized binding and pinned commit",
        ));
    }
    let namespace = content_hash(
        serde_json::to_string(&(&binding.scope, &binding.repo_id, &generation.id))
            .map_err(|_| invalid("generation namespace could not be serialized"))?,
    );
    let stamp_meta = metadata(binding, generation);
    let chunker = CodeSymbolChunker;
    let ingestor = KnowledgeIngestor::new(chunker);
    let mut documents = Vec::new();
    let mut lineage: BTreeMap<String, CodeLineage> = BTreeMap::new();
    let mut paths = HashSet::new();
    let mut ast_inputs = Vec::new();
    let mut skipped_oversized: usize = 0;
    for file in committed.files {
        let path = file
            .path
            .to_str()
            .ok_or_else(|| invalid("committed path is not UTF-8"))?;
        if path.is_empty()
            || file
                .path
                .components()
                .any(|p| !matches!(p, std::path::Component::Normal(_)))
            || !paths.insert(path)
        {
            return Err(invalid("invalid or duplicate committed source path"));
        }
        if is_secret_file(path)
            || options.scan_filter.is_denylisted(path)
            || classify_file(path) != Some(FileKind::Code)
        {
            continue;
        }
        // Per-file overflow at STAGING skips the file — parity with the legacy
        // checkout scanner (which skipped oversized files) and with the
        // acquisition stage's skip-with-notice. Real repos carry vendored/
        // generated blobs in the 1–2 MiB band (e.g. tree-sitter parser.c);
        // failing the whole generation on them regressed indexability. The
        // skip is surfaced so the caller can report it.
        if file.bytes.len() as u64 > options.max_bytes() {
            skipped_oversized += 1;
            continue;
        }
        let text =
            std::str::from_utf8(&file.bytes).map_err(|_| invalid("selected code is not UTF-8"))?;
        if text.trim().is_empty() {
            continue;
        }
        let extension = path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
        let candidates = chunker.chunk_for_ext(text, extension)?;
        let mut records = ingestor.assemble_with_candidates(
            DocumentIngestRequest {
                source_kind: SourceKind::GitRepository,
                source_name: format!("code-generation:{namespace}"),
                scope: binding.scope.clone(),
                document_kind: SourceDocumentKind::Code,
                document: DocumentMetadata {
                    path: Some(path.into()),
                    version: Some(generation.commit_oid.clone()),
                    ..Default::default()
                },
                text: text.into(),
                policy: options.policy.clone(),
                actor: options.actor.clone(),
                stable_source_key: Some(format!("bound:{namespace}")),
                repository_key: Some(binding.repo_id.0.clone()),
                git_ref: Some(binding.branch_ref.clone()),
                source_metadata: Some(stamp_meta.clone()),
            },
            candidates,
        )?;
        stamp(&mut records.document.metadata, &stamp_meta);
        for chunk in &mut records.chunks {
            stamp(&mut chunk.metadata, &stamp_meta);
        }
        let mut graph =
            GraphExtractor::new().extract(&records.source, &records.document, &records.chunks)?;
        stamp(&mut graph.graph.metadata, &stamp_meta);
        for entity in &mut graph.entities {
            stamp(&mut entity.metadata, &stamp_meta);
        }
        for (index, entities) in &graph.chunk_entities {
            if let Some(chunk) = records.chunks.get_mut(*index) {
                chunk.entities = entities.clone();
            }
        }
        let contributor = records.document.id.to_string();
        // Durable canonical spans (final r5 finding 2): each staged chunk's
        // line span is persisted WITH its lineage — the revision-bound span
        // descriptor the exact-read window later validates mutable claims
        // against. Records without a single canonical span (source, graph,
        // relationships, documents) carry `None`.
        let chunk_spans: std::collections::BTreeMap<String, engram_domain::CanonicalSpan> = records
            .chunks
            .iter()
            .filter_map(|chunk| {
                let location = chunk.location.as_ref()?;
                let span = engram_domain::CanonicalSpan {
                    start_line: location.start_line?,
                    end_line: location.end_line?,
                };
                Some((chunk.id.to_string(), span))
            })
            .collect();
        let ids = std::iter::once(records.source.id.to_string())
            .chain(std::iter::once(contributor.clone()))
            .chain(records.chunks.iter().map(|c| c.id.to_string()))
            .chain(std::iter::once(graph.graph.id.to_string()))
            .chain(graph.entities.iter().map(|e| e.id.to_string()))
            .chain(graph.relationships.iter().map(|e| e.id.to_string()));
        for id in ids {
            let value = lineage.entry(id.clone()).or_insert_with(|| CodeLineage {
                repo_id: binding.repo_id.clone(),
                generation_id: generation.id.clone(),
                source_path: path.into(),
                contributors: Vec::new(),
                span: None,
            });
            if value.source_path != path {
                value.source_path = ".".into();
            }
            // A chunk keeps its own canonical span; a record seen under a
            // second path (the "." collapse) keeps no single span.
            if value.source_path != "." {
                value.span = chunk_spans.get(&id).copied();
            }
            if !value.contributors.contains(&contributor) {
                value.contributors.push(contributor.clone());
            }
        }
        let source = IndexedSource {
            repo_id: binding.repo_id.clone(),
            generation_id: generation.id.clone(),
            commit_oid: generation.commit_oid.clone(),
            path: path.into(),
            content_digest: content_hash(&file.bytes)
                .trim_start_matches("sha256:")
                .into(),
        };
        source
            .validate()
            .map_err(|_| invalid("invalid indexed source descriptor"))?;
        ast_inputs.push((path.to_owned(), text.to_owned()));
        documents.push(StagedDocument {
            records,
            graph,
            source,
        });
    }
    // Keep CodeSymbolChunker as the identity/span source. A separate parser
    // only contributes call edges, matching the legacy scanner's AST path.
    let ts_chunker = crate::TreeSitterChunker::new().ok().map(|mut chunker| {
        engram_lang::register_all(&mut chunker);
        chunker
    });
    let entity_names: HashSet<String> = documents
        .iter()
        .flat_map(|document| {
            document
                .graph
                .entities
                .iter()
                .map(|entity| entity.name.clone())
        })
        .collect();
    let repo_context = engram_code::parser::repo_context::RepoContext::empty();
    let mut skipped_ast_call_extraction: usize = 0;
    if let Some(ts_chunker) = ts_chunker.as_ref() {
        for (document, (path, text)) in documents.iter_mut().zip(&ast_inputs) {
            let extension = path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            if !ts_chunker.supports(extension) {
                continue;
            }
            let calls =
                match ts_chunker.extract_calls(text, extension, &entity_names, &repo_context) {
                    Ok(calls) => calls,
                    Err(_) => {
                        skipped_ast_call_extraction += 1;
                        continue;
                    }
                };
            let ast_graph = GraphExtractor::new().extract_with_calls(
                &document.records.source,
                &document.records.document,
                &document.records.chunks,
                Some(&calls),
                None,
                None,
            )?;
            for relationship in &document.graph.relationships {
                lineage.remove(&relationship.id.to_string());
            }
            let contributor = document.records.document.id.to_string();
            for relationship in &ast_graph.relationships {
                lineage.insert(
                    relationship.id.to_string(),
                    CodeLineage {
                        repo_id: binding.repo_id.clone(),
                        generation_id: generation.id.clone(),
                        source_path: document.source.path.clone(),
                        contributors: vec![contributor.clone()],
                        span: None,
                    },
                );
            }
            document.graph.relationships = ast_graph.relationships;
        }
    } else {
        skipped_ast_call_extraction = documents.len();
    }
    // Resolve only after every extracted entity is registered so the result is
    // independent of committed-file order and can bind cross-file references.
    let mut symbol_index = engram_code::SymbolIndex::new();
    for document in &documents {
        register_entities(
            &mut symbol_index,
            &document.graph.entities,
            Some(binding.repo_id.0.as_str()),
            Some(document.source.path.as_str()),
        );
    }
    let mut skipped_unresolved: usize = 0;
    for document in &mut documents {
        // Bound staging has no unresolved-reference ledger surface yet, so
        // retain unresolved relationships as name-only and report their count.
        skipped_unresolved += resolve_call_refs(
            &symbol_index,
            &mut document.graph.relationships,
            Some(binding.repo_id.0.as_str()),
            Some(document.source.path.as_str()),
        )
        .len();
    }
    for value in lineage.values_mut() {
        value.contributors.sort();
        value
            .validate()
            .map_err(|_| invalid("invalid staged contributor lineage"))?;
    }
    if skipped_oversized > 0 {
        eprintln!(
            "engram-ingest: skipped {skipped_oversized} code file(s) exceeding the staging per-file bound; they are not in the bound index"
        );
    }
    if skipped_unresolved > 0 {
        eprintln!(
            "engram-ingest: skipped persistence of {skipped_unresolved} unresolved code reference(s); the bound staging ledger is deferred"
        );
    }
    if skipped_ast_call_extraction > 0 {
        eprintln!(
            "engram-ingest: skipped AST call extraction for {skipped_ast_call_extraction} code file(s); retained co-occurrence relationships"
        );
    }
    Ok(PreparedGeneration {
        binding: binding.clone(),
        generation: generation.clone(),
        documents,
        lineage,
    })
}

/// Write only prepared IDs; never delete or reconcile the active generation.
/// Failure leaves attempt-owned partial writes for a later receipt-aware retry.
pub async fn write_prepared_generation<R>(
    store: &R,
    prepared: &PreparedGeneration,
) -> CoreResult<()>
where
    R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized,
{
    for document in &prepared.documents {
        store.put_source(document.records.source.clone()).await?;
        store
            .put_document(document.records.document.clone())
            .await?;
        for chunk in &document.records.chunks {
            store.put_chunk(chunk.clone()).await?;
        }
        store.put_graph(document.graph.graph.clone()).await?;
        for entity in &document.graph.entities {
            store.put_entity(entity.clone()).await?;
        }
        for relationship in &document.graph.relationships {
            store.put_relationship(relationship.clone()).await?;
        }
        if !document.graph.unresolved.is_empty() {
            store
                .put_unresolved_refs(document.graph.unresolved.clone())
                .await?;
        }
    }
    Ok(())
}
