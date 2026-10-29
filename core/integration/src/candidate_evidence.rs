//! Canonical candidate evidence used only while code eligibility is unavailable.

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::{
    EvidenceTargetType, KnowledgeChunkKind, RetrievalRequest, RetrievalResult, RetrievalTargetType,
};
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository};
use engram_memory::MemoryRepository;
use engram_retrieval::{CandidateEvidence, CandidateEvidenceResolver};
use engram_runtime::{CoreResult, PolicyAuthorizer};

/// Reads each capability's canonical scoped record before permitting degraded recall.
pub struct RepositoryCandidateEvidence {
    memory: Arc<dyn MemoryRepository>,
    beliefs: Arc<dyn BeliefRepository>,
    knowledge: Arc<dyn KnowledgeRepository>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    ownership: Option<Arc<dyn CodeBindingRepository>>,
}

impl RepositoryCandidateEvidence {
    pub fn new(
        memory: Arc<dyn MemoryRepository>,
        beliefs: Arc<dyn BeliefRepository>,
        knowledge: Arc<dyn KnowledgeRepository>,
        authorizer: Arc<dyn PolicyAuthorizer>,
        ownership: Option<Arc<dyn CodeBindingRepository>>,
    ) -> Self {
        Self {
            memory,
            beliefs,
            knowledge,
            authorizer,
            ownership,
        }
    }
}

fn code_chunk(kind: KnowledgeChunkKind) -> bool {
    matches!(
        kind,
        KnowledgeChunkKind::CodeSymbol
            | KnowledgeChunkKind::CodeBlock
            | KnowledgeChunkKind::DiffHunk
    )
}

/// A dependency reference is not trusted by its target kind alone.
/// A direct code target is dependent, while a reference to another derived
/// record is unknown until that record's own canonical evidence is resolved.
fn dependency_evidence(provenance: &engram_domain::Provenance) -> CandidateEvidence {
    let refs = provenance.evidence.iter().chain(
        provenance
            .derivations
            .iter()
            .flat_map(|d| d.input_refs.iter()),
    );
    let mut unknown = false;
    for reference in refs {
        match reference.target_type {
            EvidenceTargetType::Chunk | EvidenceTargetType::Entity | EvidenceTargetType::Source => {
                return CandidateEvidence::CodeDependent;
            }
            EvidenceTargetType::Memory | EvidenceTargetType::Belief => unknown = true,
            _ => {}
        }
    }
    if unknown {
        CandidateEvidence::Unknown
    } else {
        CandidateEvidence::Ordinary
    }
}

#[async_trait]
impl CandidateEvidenceResolver for RepositoryCandidateEvidence {
    async fn evidence(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<CandidateEvidence> {
        match candidate.target_type {
            RetrievalTargetType::Memory => match self
                .memory
                .get_memory(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self
                        .authorizer
                        .can_retrieve(&request.requester, &record.scope, &record.policy)
                        .is_ok()
                        && record.content.text == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    Ok(dependency_evidence(&record.provenance))
                }
                Some(_) | None => Ok(CandidateEvidence::Unknown),
            },
            RetrievalTargetType::Belief => match self
                .beliefs
                .get_belief_by_id(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self
                        .authorizer
                        .can_retrieve(&request.requester, &record.scope, &record.policy)
                        .is_ok()
                        && record.content == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    Ok(dependency_evidence(&record.provenance))
                }
                Some(_) | None => Ok(CandidateEvidence::Unknown),
            },
            RetrievalTargetType::Chunk => match self
                .knowledge
                .get_chunk(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record) => match self
                    .knowledge
                    .get_source(&record.source_id, &request.scope)
                    .await?
                {
                    Some(source)
                        if self
                            .authorizer
                            .can_retrieve(&request.requester, &source.scope, &source.policy)
                            .is_ok()
                            && self
                                .authorizer
                                .can_retrieve(&request.requester, &source.scope, &record.policy)
                                .is_ok()
                            && record.text == candidate.content
                            && record.provenance == candidate.provenance
                            && record.policy == candidate.policy =>
                    {
                        if code_chunk(record.kind) {
                            Ok(CandidateEvidence::CodeDependent)
                        } else {
                            Ok(dependency_evidence(&record.provenance))
                        }
                    }
                    Some(_) | None => Ok(CandidateEvidence::Unknown),
                },
                None => Ok(CandidateEvidence::Unknown),
            },
            RetrievalTargetType::Entity => match self
                .knowledge
                .get_entity(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record) if candidate.provenance == record.provenance => {
                    let Some(ownership) = &self.ownership else {
                        return Ok(CandidateEvidence::Unknown);
                    };
                    let lineage = ownership.target_lineage(&request.scope).await?;
                    if lineage
                        .iter()
                        .any(|entry| entry.record_id == candidate.target_id)
                    {
                        Ok(CandidateEvidence::CodeDependent)
                    } else {
                        Ok(CandidateEvidence::Unknown)
                    }
                }
                Some(_) | None => Ok(CandidateEvidence::Unknown),
            },
            _ => Ok(CandidateEvidence::Unknown),
        }
    }
}
