//! Prepare and persist attempt-owned code records without reconciling the published generation.
use crate::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, ExtractedGraph, FileKind,
    GraphExtractor, IngestedKnowledge, KnowledgeIngestor, ScanOptions, classify_file, content_hash,
    is_secret_file,
};
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use std::collections::BTreeMap;

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
    let mut paths = std::collections::HashSet::new();
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
        if file.bytes.len() as u64 > options.max_bytes() {
            return Err(invalid(
                "selected code exceeds the configured file size bound",
            ));
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
        let ids = std::iter::once(records.source.id.to_string())
            .chain(std::iter::once(contributor.clone()))
            .chain(records.chunks.iter().map(|c| c.id.to_string()))
            .chain(std::iter::once(graph.graph.id.to_string()))
            .chain(graph.entities.iter().map(|e| e.id.to_string()))
            .chain(graph.relationships.iter().map(|e| e.id.to_string()));
        for id in ids {
            let value = lineage.entry(id).or_insert_with(|| CodeLineage {
                repo_id: binding.repo_id.clone(),
                generation_id: generation.id.clone(),
                source_path: path.into(),
                contributors: Vec::new(),
            });
            if value.source_path != path {
                value.source_path = ".".into();
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
        documents.push(StagedDocument {
            records,
            graph,
            source,
        });
    }
    for value in lineage.values_mut() {
        value.contributors.sort();
        value
            .validate()
            .map_err(|_| invalid("invalid staged contributor lineage"))?;
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
