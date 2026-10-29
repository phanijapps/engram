//! Canonical candidate evidence used only while code eligibility is unavailable.
//!
//! Every classification resolves the candidate's canonical record through the
//! OWNING repositories under the request scope, authorizes the read through
//! the same configured [`PolicyAuthorizer`] the owning retrieval path uses
//! (`request.requester` + the record's canonical scope/policy — policy
//! equality alone is never authorization), binds the candidate payload to the
//! canonical bytes/revision, and then walks the record's canonical dependency
//! chain (provenance evidence + derivation inputs) with CYCLE and DEPTH
//! bounds. A record is `CodeDependent` when its own kind or durable target
//! lineage is code, or when any reachable dependency is. A record is
//! positively `Ordinary` only when its canonical origin and every reachable
//! dependency resolve through authorized ordinary records. Mismatched
//! payloads, foreign/missing records, unauthorized reads, cycles on the
//! resolution path that cannot be settled, and depth/work overruns all
//! degrade to `Unknown` — ambiguity never becomes ordinary evidence.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::{
    AllowedUse, DeleteMode, EvidenceTargetType, KnowledgeChunkKind, Policy, Retention,
    RetrievalRequest, RetrievalResult, RetrievalTargetType, Sensitivity, Visibility,
};
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository};
use engram_memory::MemoryRepository;
use engram_retrieval::{CandidateEvidence, CandidateEvidenceResolver};
use engram_runtime::{CoreResult, PolicyAuthorizer};

/// Maximum dependency-chain depth evaluated per candidate. Deeper ambiguity
/// degrades to `Unknown` rather than an unbounded walk.
const MAX_DEPENDENCY_DEPTH: usize = 8;
/// Maximum canonical records resolved per candidate evaluation (work bound).
const MAX_DEPENDENCY_RECORDS: usize = 256;

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

    /// The owning retrieval decision for one canonical record: the request's
    /// authenticated requester against the record's own scope and policy.
    fn authorize(
        &self,
        request: &RetrievalRequest,
        scope: &engram_domain::Scope,
        policy: &Policy,
    ) -> bool {
        self.authorizer
            .can_retrieve(&request.requester, scope, policy)
            .is_ok()
    }

    /// True when `record_id` carries durable code lineage in the request
    /// scope. An unconfigured ownership reader cannot establish the negative
    /// and callers treat that as `Unknown`.
    async fn in_code_lineage(
        &self,
        record_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<bool>> {
        let Some(ownership) = &self.ownership else {
            return Ok(None);
        };
        let lineage = ownership.target_lineage(&request.scope).await?;
        Ok(Some(
            lineage.iter().any(|entry| entry.record_id == record_id),
        ))
    }

    /// Bounded, cycle-safe dependency evaluation over canonical records.
    ///
    /// Aggregate semantics: any reachable code-dependent record makes the
    /// candidate `CodeDependent`; otherwise any unresolved (missing, foreign,
    /// unauthorized, over-bound) dependency makes it `Unknown`; only a fully
    /// resolved ordinary chain yields `Ordinary`. The walk is ITERATIVE (an
    /// explicit provenance stack — async recursion would require boxing and
    /// an unbounded future). The `resolved` set both bounds work and breaks
    /// cycles: a record visited earlier in the same walk has already
    /// contributed its classification, so a back-edge adds nothing new (a
    /// pure ordinary cycle stays ordinary; code anywhere in the cycle still
    /// surfaces through the other edge's fold).
    async fn dependencies(
        &self,
        provenance: &engram_domain::Provenance,
        request: &RetrievalRequest,
        resolved: &mut BTreeSet<(u8, String)>,
        depth: usize,
    ) -> CoreResult<CandidateEvidence> {
        let mut aggregate = CandidateEvidence::Ordinary;
        let mut stack: Vec<(engram_domain::Provenance, usize)> = vec![(provenance.clone(), depth)];
        while let Some((frame, frame_depth)) = stack.pop() {
            if frame_depth >= MAX_DEPENDENCY_DEPTH {
                aggregate = upgrade(aggregate, CandidateEvidence::Unknown);
                continue;
            }
            let references = frame.evidence.iter().chain(
                frame
                    .derivations
                    .iter()
                    .flat_map(|derivation| derivation.input_refs.iter()),
            );
            for reference in references {
                let (classification, continuation) = self
                    .reference_evidence(reference, request, resolved)
                    .await?;
                aggregate = upgrade(aggregate, classification);
                if let Some(next) = continuation {
                    stack.push((next, frame_depth + 1));
                }
                if aggregate == CandidateEvidence::CodeDependent {
                    return Ok(CandidateEvidence::CodeDependent);
                }
            }
        }
        Ok(aggregate)
    }

    /// Classifies one provenance reference by resolving its canonical record.
    /// Returns the reference's terminal classification plus, when the record
    /// itself is ordinary-so-far and its own provenance still needs walking,
    /// that provenance for the caller's stack.
    async fn reference_evidence(
        &self,
        reference: &engram_domain::EvidenceRef,
        request: &RetrievalRequest,
        resolved: &mut BTreeSet<(u8, String)>,
    ) -> CoreResult<(CandidateEvidence, Option<engram_domain::Provenance>)> {
        match reference.target_type {
            // Code-capable record categories: the reference must resolve to
            // an authorized canonical record, which is then classified by
            // its own kind, durable lineage and dependencies.
            EvidenceTargetType::Chunk => {
                let Some(id) = reference.target_id.as_deref() else {
                    // A chunk reference that names no canonical record cannot
                    // be settled while the binding authority is unavailable.
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Chunk, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_chunk(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                // Chunks authorize through their canonical source's scope
                // (the owning scoped read pair the direct path uses).
                let Some(source) = self
                    .knowledge
                    .get_source(&record.source_id, &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self.authorize(request, &source.scope, &record.policy) {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                if code_chunk(record.kind)
                    || matches!(self.in_code_lineage(id, request).await?, Some(true))
                {
                    return Ok((CandidateEvidence::CodeDependent, None));
                }
                resolved.insert(target_key(
                    &EvidenceTargetType::Source,
                    &record.source_id.to_string(),
                ));
                Ok((CandidateEvidence::Ordinary, Some(record.provenance.clone())))
            }
            EvidenceTargetType::Entity => {
                let Some(id) = reference.target_id.as_deref() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Entity, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_entity(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self.authorize(request, &record.scope, &graph_default_policy()) {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                if entity_is_code_kind(&record) {
                    return Ok((CandidateEvidence::CodeDependent, None));
                }
                match self.in_code_lineage(id, request).await? {
                    Some(true) => Ok((CandidateEvidence::CodeDependent, None)),
                    // Positively established ordinary entity origin: continue
                    // through the entity's own provenance.
                    Some(false) => {
                        Ok((CandidateEvidence::Ordinary, Some(record.provenance.clone())))
                    }
                    None => Ok((CandidateEvidence::Unknown, None)),
                }
            }
            EvidenceTargetType::Source => {
                let Some(id) = reference.target_id.as_deref() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Source, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_source(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self.authorize(request, &record.scope, &record.policy) {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                // A repository-bound source has typed code ownership that the
                // unavailable eligibility authority would settle; an ordinary
                // (non-repository) source is a positive ordinary origin.
                if record.repository_key.is_some() {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                Ok((CandidateEvidence::Ordinary, None))
            }
            // Derived-record categories: resolve the canonical record and
            // continue the walk through its own provenance.
            EvidenceTargetType::Memory | EvidenceTargetType::Belief => {
                let Some(id) = reference.target_id.clone() else {
                    return Ok((CandidateEvidence::Ordinary, None));
                };
                if !resolved.insert(target_key(&reference.target_type, &id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                match reference.target_type {
                    EvidenceTargetType::Memory => {
                        let Some(record) = self
                            .memory
                            .get_memory(&id.clone().into(), &request.scope)
                            .await?
                        else {
                            return Ok((CandidateEvidence::Unknown, None));
                        };
                        if !self.authorize(request, &record.scope, &record.policy) {
                            return Ok((CandidateEvidence::Unknown, None));
                        }
                        Ok((CandidateEvidence::Ordinary, Some(record.provenance.clone())))
                    }
                    _ => {
                        let Some(record) = self
                            .beliefs
                            .get_belief_by_id(&id.clone().into(), &request.scope)
                            .await?
                        else {
                            return Ok((CandidateEvidence::Unknown, None));
                        };
                        if !self.authorize(request, &record.scope, &record.policy) {
                            return Ok((CandidateEvidence::Unknown, None));
                        }
                        Ok((CandidateEvidence::Ordinary, Some(record.provenance.clone())))
                    }
                }
            }
            // Categories outside the code-binding universe: an event,
            // concept, URL, document or relationship reference cannot carry
            // durable code lineage (code capability is established only on
            // chunk/entity records through the binding store).
            _ => Ok((CandidateEvidence::Ordinary, None)),
        }
    }
}

/// Upgrade the aggregate classification monoid: code dependence dominates,
/// unresolved evidence degrades ordinary (never the other way round).
fn upgrade(current: CandidateEvidence, next: CandidateEvidence) -> CandidateEvidence {
    match (current, next) {
        (CandidateEvidence::CodeDependent, _) | (_, CandidateEvidence::CodeDependent) => {
            CandidateEvidence::CodeDependent
        }
        (CandidateEvidence::Unknown, _) | (_, CandidateEvidence::Unknown) => {
            CandidateEvidence::Unknown
        }
        (CandidateEvidence::Ordinary, CandidateEvidence::Ordinary) => CandidateEvidence::Ordinary,
    }
}

/// Compact resolution key for cycle/work bounding. `EvidenceTargetType` is a
/// wire enum without `Ord`/`Hash`, so the walk keys on a stable per-variant
/// discriminant plus the target id.
fn target_key(target_type: &EvidenceTargetType, id: &str) -> (u8, String) {
    let discriminant = match target_type {
        EvidenceTargetType::Memory => 1,
        EvidenceTargetType::Event => 2,
        EvidenceTargetType::Source => 3,
        EvidenceTargetType::Document => 4,
        EvidenceTargetType::Chunk => 5,
        EvidenceTargetType::Entity => 6,
        EvidenceTargetType::Concept => 7,
        EvidenceTargetType::Url => 8,
        EvidenceTargetType::Relationship => 9,
        EvidenceTargetType::Belief => 10,
    };
    (discriminant, id.to_owned())
}

fn code_chunk(kind: KnowledgeChunkKind) -> bool {
    matches!(
        kind,
        KnowledgeChunkKind::CodeSymbol
            | KnowledgeChunkKind::CodeBlock
            | KnowledgeChunkKind::DiffHunk
    )
}

/// Symbol/structural entity kinds. An entity of a code kind is code even
/// without durable lineage (legacy unbound code) and is denied while the
/// binding authority is unavailable.
fn entity_is_code_kind(entity: &engram_domain::KnowledgeEntity) -> bool {
    matches!(
        entity.kind,
        engram_domain::EntityKind::Class
            | engram_domain::EntityKind::Function
            | engram_domain::EntityKind::Method
            | engram_domain::EntityKind::Struct
            | engram_domain::EntityKind::Interface
            | engram_domain::EntityKind::Trait
            | engram_domain::EntityKind::TypeAlias
            | engram_domain::EntityKind::Enum
    )
}

/// Entities carry no canonical policy field; the owning graph lane stamps
/// this default on entity candidates, so the evidence path authorizes the
/// same shape (the owning decision, not a resolver-invented bypass).
fn graph_default_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

#[async_trait]
impl CandidateEvidenceResolver for RepositoryCandidateEvidence {
    async fn evidence(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<CandidateEvidence> {
        let mut resolved = BTreeSet::new();
        match candidate.target_type {
            RetrievalTargetType::Memory => match self
                .memory
                .get_memory(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self.authorize(request, &record.scope, &record.policy)
                        && record.content.text == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    self.dependencies(&record.provenance, request, &mut resolved, 0)
                        .await
                }
                Some(_) | None => Ok(CandidateEvidence::Unknown),
            },
            RetrievalTargetType::Belief => match self
                .beliefs
                .get_belief_by_id(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self.authorize(request, &record.scope, &record.policy)
                        && record.content == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    self.dependencies(&record.provenance, request, &mut resolved, 0)
                        .await
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
                        if self.authorize(request, &source.scope, &source.policy)
                            && self.authorize(request, &source.scope, &record.policy)
                            && record.text == candidate.content
                            && record.provenance == candidate.provenance
                            && record.policy == candidate.policy =>
                    {
                        resolved.insert(target_key(
                            &EvidenceTargetType::Source,
                            &record.source_id.to_string(),
                        ));
                        if code_chunk(record.kind)
                            || matches!(
                                self.in_code_lineage(candidate.target_id.as_str(), request)
                                    .await?,
                                Some(true)
                            )
                        {
                            return Ok(CandidateEvidence::CodeDependent);
                        }
                        self.dependencies(&record.provenance, request, &mut resolved, 0)
                            .await
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
                Some(record)
                    if record.name == candidate.content
                        && record.provenance == candidate.provenance =>
                {
                    if !self.authorize(request, &record.scope, &graph_default_policy()) {
                        return Ok(CandidateEvidence::Unknown);
                    }
                    if entity_is_code_kind(&record) {
                        return Ok(CandidateEvidence::CodeDependent);
                    }
                    // Without the canonical ownership reader the absence of
                    // lineage cannot be established positively — Unknown, not
                    // ordinary (the default unconfigured boundary).
                    match self
                        .in_code_lineage(candidate.target_id.as_str(), request)
                        .await?
                    {
                        Some(true) => Ok(CandidateEvidence::CodeDependent),
                        Some(false) => {
                            self.dependencies(&record.provenance, request, &mut resolved, 0)
                                .await
                        }
                        None => Ok(CandidateEvidence::Unknown),
                    }
                }
                Some(_) | None => Ok(CandidateEvidence::Unknown),
            },
            _ => Ok(CandidateEvidence::Unknown),
        }
    }
}
