//! Fenced metadata enrichment after pure graph extraction.
use crate::ExtractedGraph;
use engram_domain::*;
use engram_knowledge::{CoreError, CoreResult, KnowledgeGraphRepository, KnowledgeRepository};

pub(crate) async fn persist<R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized>(
    repository: &R,
    source: &KnowledgeSource,
    document: &SourceDocument,
    chunks: &[KnowledgeChunk],
    extracted: &ExtractedGraph,
) -> CoreResult<()> {
    let head = if let Some(port) = repository.document_heads() {
        Some(crate::document_generation::enrichment(port, source, document, chunks).await?)
    } else {
        None
    };
    let persisted = async {
        persist_derived(repository, head.as_ref(), extracted).await?;
        // Re-persist chunks with their entity refs stamped (Part A).
        for (chunk_idx, entity_refs) in &extracted.chunk_entities {
            if let Some(chunk) = chunks.get(*chunk_idx) {
                let mut updated = chunk.clone();
                updated.entities = entity_refs.clone();
                if let Some(head) = &head {
                    repository
                        .document_heads()
                        .unwrap()
                        .write_document_record(
                            &head.key,
                            &head.attempt_id,
                            engram_knowledge::DocumentHeadWrite::Chunk(updated),
                        )
                        .await?;
                } else {
                    repository.put_chunk(updated).await?;
                }
            }
        }
        if let Some(mut active) = head.clone() {
            active.state = engram_domain::CodeSourceState::Active;
            repository
                .document_heads()
                .unwrap()
                .activate_document_head(active)
                .await?;
        }
        Ok::<(), CoreError>(())
    }
    .await;
    if let Err(error) = persisted {
        if let Some(head) = head {
            let _ = repository
                .document_heads()
                .unwrap()
                .fail_document_head(&head.key, &head.attempt_id)
                .await;
        }
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn persist_derived<R: KnowledgeRepository + KnowledgeGraphRepository + ?Sized>(
    repository: &R,
    head: Option<&DocumentHead>,
    extracted: &ExtractedGraph,
) -> CoreResult<()> {
    if let Some(head) = head {
        let port = repository.document_heads().unwrap();
        use engram_knowledge::DocumentHeadWrite as Write;
        port.write_document_record(
            &head.key,
            &head.attempt_id,
            Write::Graph(extracted.graph.clone()),
        )
        .await?;
        for entity in &extracted.entities {
            if entity.graph_id.is_none() {
                let file = CodeFileKey {
                    scope: head.key.scope.clone(),
                    repository: head.key.repository.clone(),
                    path: head.key.source_key.clone(),
                };
                if entity.kind == EntityKind::Repository
                    && file.is_valid()
                    && let Some(context) = repository.code_sources()
                {
                    context
                        .ensure_repository_context(&file, entity.clone())
                        .await?;
                } else {
                    repository.put_entity(entity.clone()).await?;
                }
            } else {
                port.write_document_record(
                    &head.key,
                    &head.attempt_id,
                    Write::Entity(entity.clone()),
                )
                .await?;
            }
        }
        for r in &extracted.relationships {
            port.write_document_record(&head.key, &head.attempt_id, Write::Relationship(r.clone()))
                .await?;
        }
        for r in &extracted.unresolved {
            port.write_document_record(
                &head.key,
                &head.attempt_id,
                Write::UnresolvedReference(r.clone()),
            )
            .await?;
        }
    } else {
        repository.put_graph(extracted.graph.clone()).await?;
        for e in &extracted.entities {
            repository.put_entity(e.clone()).await?;
        }
        for r in &extracted.relationships {
            repository.put_relationship(r.clone()).await?;
        }
        if !extracted.unresolved.is_empty() {
            repository
                .put_unresolved_refs(extracted.unresolved.clone())
                .await?;
        }
    }
    Ok(())
}
