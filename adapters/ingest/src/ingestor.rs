//! Knowledge record assembly and repository writes.
//!
//! This module owns deterministic conversion from caller-supplied text into
//! `KnowledgeSource`, `SourceDocument`, and `KnowledgeChunk` records. It does
//! not read files, clone repositories, embed text, or turn source facts into
//! memory records.

use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeRepository};

use crate::{
    chunker::Chunker,
    hash::content_hash,
    request::{DocumentIngestRequest, DocumentMetadata},
};

/// Records created by one document ingestion operation.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestedKnowledge {
    pub source: KnowledgeSource,
    pub document: SourceDocument,
    pub chunks: Vec<KnowledgeChunk>,
}

/// Deterministic text ingestor over a caller-supplied chunker.
#[derive(Debug, Clone)]
pub struct KnowledgeIngestor<C> {
    chunker: C,
}

impl<C> KnowledgeIngestor<C>
where
    C: Chunker,
{
    /// Creates an ingestor using the supplied chunker.
    ///
    /// The chunker is injected so tests and future source-specific adapters can
    /// change chunk boundaries without changing persistence orchestration.
    pub fn new(chunker: C) -> Self {
        Self { chunker }
    }

    /// Creates knowledge records and persists them through `KnowledgeRepository`.
    ///
    /// IDs and hashes are derived from source/document/chunk content so the
    /// same input can be reingested deterministically. Embeddings are left empty
    /// because vectorization belongs to a later adapter stage.
    pub async fn ingest<R>(
        &self,
        repository: &R,
        request: DocumentIngestRequest,
    ) -> CoreResult<IngestedKnowledge>
    where
        R: KnowledgeRepository + ?Sized,
    {
        validate_request(&request)?;

        let document_hash = content_hash(&request.text);
        let source_id = source_id(&request);
        let document_id = document_id(&source_id, &request.document, &document_hash);
        let now = Utc::now();
        let provenance = Provenance {
            source: request.source_name.clone(),
            actor: request.actor.clone(),
            observed_at: now,
            evidence: Vec::new(),
            derivations: vec![DerivationRef {
                kind: DerivationKind::Ingestion,
                model: None,
                prompt_hash: None,
                input_refs: Vec::new(),
                created_at: now,
            }],
            confidence: Some(1.0),
            method: Some("deterministic_text_ingestion".to_owned()),
        };
        let source = KnowledgeSource {
            id: source_id.clone(),
            kind: request.source_kind,
            scope: request.scope,
            name: request.source_name,
            uri: request.document.uri.clone(),
            version: request.document.version.clone(),
            policy: request.policy.clone(),
            provenance: provenance.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        };
        let document = SourceDocument {
            id: document_id.clone(),
            source_id: source_id.clone(),
            kind: request.document_kind,
            uri: request.document.uri,
            path: request.document.path.clone(),
            title: request.document.title,
            mime_type: request.document.mime_type,
            language: request.document.language,
            version: request.document.version,
            content_hash: document_hash.clone(),
            provenance: provenance.clone(),
            policy: request.policy.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        };
        let chunks = self
            .chunker
            .chunk(&request.text)?
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let chunk_hash = content_hash(&candidate.text);
                KnowledgeChunk {
                    id: chunk_id(&document_id, index, &chunk_hash),
                    document_id: document_id.clone(),
                    source_id: source_id.clone(),
                    kind: candidate.kind,
                    text: candidate.text,
                    summary: None,
                    location: merge_location_path(
                        candidate.location,
                        request.document.path.as_deref(),
                    ),
                    entities: Vec::new(),
                    concepts: Vec::new(),
                    embedding_refs: Vec::new(),
                    content_hash: chunk_hash,
                    provenance: provenance.clone(),
                    policy: request.policy.clone(),
                    created_at: now,
                    updated_at: None,
                    metadata: None,
                }
            })
            .collect::<Vec<_>>();

        let source = repository.put_source(source).await?;
        let document = repository.put_document(document).await?;
        let mut persisted_chunks = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            persisted_chunks.push(repository.put_chunk(chunk).await?);
        }

        Ok(IngestedKnowledge {
            source,
            document,
            chunks: persisted_chunks,
        })
    }
}

fn validate_request(request: &DocumentIngestRequest) -> CoreResult<()> {
    if request.source_name.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "source_name must not be empty".to_owned(),
        });
    }
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant must not be empty".to_owned(),
        });
    }
    if request.text.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "document text must not be empty".to_owned(),
        });
    }
    Ok(())
}

fn source_id(request: &DocumentIngestRequest) -> SourceId {
    Id::from(format!(
        "source-{}",
        content_hash(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{:?}",
            request.scope.tenant,
            request.document.uri.as_deref().unwrap_or_default(),
            request.source_name,
            request.source_kind
        ))
        .trim_start_matches("sha256:")
    ))
}

fn document_id(
    source_id: &SourceId,
    metadata: &DocumentMetadata,
    document_hash: &str,
) -> DocumentId {
    Id::from(format!(
        "document-{}",
        content_hash(format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            source_id,
            metadata.path.as_deref().unwrap_or_default(),
            metadata.version.as_deref().unwrap_or_default(),
            document_hash
        ))
        .trim_start_matches("sha256:")
    ))
}

fn chunk_id(document_id: &DocumentId, index: usize, chunk_hash: &str) -> ChunkId {
    Id::from(format!(
        "chunk-{}",
        content_hash(format!("{document_id}\u{1f}{index}\u{1f}{chunk_hash}"))
            .trim_start_matches("sha256:")
    ))
}

fn merge_location_path(
    location: Option<SourceLocation>,
    path: Option<&str>,
) -> Option<SourceLocation> {
    location.map(|mut location| {
        if location.path.is_none() {
            location.path = path.map(str::to_owned);
        }
        location
    })
}

use chrono::Utc;
