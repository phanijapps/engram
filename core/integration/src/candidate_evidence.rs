//! Canonical candidate evidence used only while code eligibility is unavailable.
//!
//! Every classification resolves the candidate's canonical record through the
//! OWNING repositories under the request scope, authorizes the read through
//! the same configured [`PolicyAuthorizer`] the owning retrieval path uses
//! (`request.requester` + the record's canonical scope/policy — policy
//! equality alone is never authorization), binds the candidate payload to the
//! canonical bytes/revision, and then walks the record's canonical dependency
//! chain with CYCLE and total WORK bounds. A record is `CodeDependent` when
//! its own kind or durable target lineage is code, or when any reachable
//! dependency is. A record is positively `Ordinary` only when its canonical
//! origin and every reachable dependency resolve through authorized ordinary
//! records. Mismatched payloads, foreign/missing records, unauthorized reads,
//! ID-less derived references, canonically unresolvable reference categories,
//! ambiguous typed origins, cycles that cannot be settled and work/depth
//! overruns all degrade to `Unknown` — ambiguity never becomes ordinary
//! evidence (corrective r4, canonical 6a/6b/6d).

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::{
    EvidenceTargetType, KnowledgeChunkKind, Policy, RetrievalRequest, RetrievalResult,
    RetrievalTargetType,
};
use engram_knowledge::{CodeBindingRepository, KnowledgeGraphRepository, KnowledgeRepository};
use engram_memory::MemoryRepository;
use engram_retrieval::{CandidateEvidence, CandidateEvidenceResolver, EvidenceClassification};
use engram_runtime::{CoreResult, PolicyAuthorizer};

/// Maximum dependency-chain depth evaluated per candidate. Deeper ambiguity
/// degrades to `Unknown` rather than an unbounded walk.
const MAX_DEPENDENCY_DEPTH: usize = 8;
/// Maximum canonical records resolved per candidate evaluation.
const MAX_DEPENDENCY_RECORDS: usize = 256;
/// TOTAL reference examinations (work) per candidate evaluation — enforced
/// BEFORE any per-reference cloning or enqueueing so neither processing nor
/// allocation grows with a hostile dependency array (canonical 6d).
const MAX_DEPENDENCY_REFERENCES: usize = 1024;

/// Reads each capability's canonical scoped record before permitting degraded recall.
pub struct RepositoryCandidateEvidence {
    memory: Arc<dyn MemoryRepository>,
    beliefs: Arc<dyn BeliefRepository>,
    knowledge: Arc<dyn KnowledgeRepository>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    ownership: Option<Arc<dyn CodeBindingRepository>>,
    graph: Option<Arc<dyn KnowledgeGraphRepository>>,
}

impl RepositoryCandidateEvidence {
    pub fn new(
        memory: Arc<dyn MemoryRepository>,
        beliefs: Arc<dyn BeliefRepository>,
        knowledge: Arc<dyn KnowledgeRepository>,
        authorizer: Arc<dyn PolicyAuthorizer>,
        ownership: Option<Arc<dyn CodeBindingRepository>>,
        graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    ) -> Self {
        Self {
            memory,
            beliefs,
            knowledge,
            authorizer,
            ownership,
            graph,
        }
    }

    /// Resolves graph-owned records through their canonical graph policy.
    /// Graphless entities and relationships are resolved ordinary facts whose
    /// domain default still flows through the configured owning authorizer.
    /// Missing graph authority, a missing graph row, or a foreign graph deny.
    async fn authorize_graph(
        &self,
        request: &RetrievalRequest,
        scope: &engram_domain::Scope,
        graph_id: Option<&engram_domain::KnowledgeGraphId>,
    ) -> CoreResult<bool> {
        let Some(graph_id) = graph_id else {
            return Ok(self.authorize(request, scope, &Policy::graph_lane_default()));
        };
        let Some(graphs) = &self.graph else {
            return Ok(false);
        };
        let Some(graph) = graphs.get_graph(graph_id, scope).await? else {
            return Ok(false);
        };
        if graph.scope != *scope {
            return Ok(false);
        }
        Ok(self.authorize(request, &graph.scope, &graph.policy))
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
    /// and callers treat that as `Unknown` (never as a negative lookup).
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
    /// unauthorized, over-bound, ID-less, unresolvable-category) dependency
    /// makes it `Unknown`; only a fully resolved ordinary chain yields
    /// `Ordinary`. The walk is ITERATIVE (an explicit provenance stack —
    /// async recursion would require boxing and an unbounded future). The
    /// `resolved` set breaks cycles: a record visited earlier in the same
    /// walk has already contributed its classification, so a back-edge adds
    /// nothing new (a pure ordinary cycle stays ordinary; code anywhere in
    /// the cycle still surfaces through the other edge's fold). The `work`
    /// counter is the TOTAL reference budget: exhaustion stops the whole
    /// walk immediately with `Unknown` — no further references are examined,
    /// cloned or enqueued (canonical 6d). When `code_dependencies` is
    /// supplied, every resolved code record's id is appended for the recipe
    /// normal-path eligibility check.
    async fn dependencies(
        &self,
        provenance: &engram_domain::Provenance,
        request: &RetrievalRequest,
        resolved: &mut BTreeSet<(u8, String)>,
        work: &mut usize,
        code_dependencies: Option<&mut Vec<String>>,
    ) -> CoreResult<CandidateEvidence> {
        let mut aggregate = CandidateEvidence::Ordinary;
        let mut collected = code_dependencies;
        let mut stack: Vec<(engram_domain::Provenance, usize)> = vec![(provenance.clone(), 0)];
        while let Some((frame, frame_depth)) = stack.pop() {
            if frame_depth >= MAX_DEPENDENCY_DEPTH {
                return Ok(CandidateEvidence::Unknown);
            }
            let references = frame.evidence.iter().chain(
                frame
                    .derivations
                    .iter()
                    .flat_map(|derivation| derivation.input_refs.iter()),
            );
            for reference in references {
                // Total work budget BEFORE any per-reference processing
                // (canonical 6d): exhaustion ends the evaluation.
                if *work >= MAX_DEPENDENCY_REFERENCES {
                    return Ok(CandidateEvidence::Unknown);
                }
                *work += 1;
                let (classification, continuation) = self
                    .reference_evidence(reference, request, resolved, work, &mut collected)
                    .await?;
                aggregate = upgrade(aggregate, classification);
                if let Some(next) = continuation {
                    if *work >= MAX_DEPENDENCY_REFERENCES
                        || resolved.len() >= MAX_DEPENDENCY_RECORDS
                    {
                        return Ok(CandidateEvidence::Unknown);
                    }
                    stack.push((next, frame_depth + 1));
                }
                // Do not stop at the first code dependency. Every reachable
                // reference must be resolved so a stale, denied, or missing
                // sibling cannot be hidden behind an eligible dependency.
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
        work: &mut usize,
        code_dependencies: &mut Option<&mut Vec<String>>,
    ) -> CoreResult<(CandidateEvidence, Option<engram_domain::Provenance>)> {
        match reference.target_type {
            // Code-capable record categories: the reference must resolve to
            // an authorized canonical record, which is then classified by
            // its own kind, durable lineage and dependencies.
            EvidenceTargetType::Chunk => {
                // An ID-less code-capable reference cannot be settled while
                // the binding authority is unavailable (canonical 6a).
                let Some(id) = reference.target_id.as_deref() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Chunk, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS || *work >= MAX_DEPENDENCY_REFERENCES {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_chunk(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                // Chunks authorize through BOTH their canonical source's
                // scope/policy AND their own policy (canonical 6a).
                let Some(source) = self
                    .knowledge
                    .get_source(&record.source_id, &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self.authorize(request, &source.scope, &source.policy)
                    || !self.authorize(request, &source.scope, &record.policy)
                {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                if code_chunk(record.kind) {
                    if let Some(collected) = code_dependencies.as_deref_mut() {
                        collected.push(id.to_owned());
                    }
                    return Ok((CandidateEvidence::CodeDependent, None));
                }
                match self.in_code_lineage(id, request).await? {
                    Some(true) => {
                        if let Some(collected) = code_dependencies.as_deref_mut() {
                            collected.push(id.to_owned());
                        }
                        Ok((CandidateEvidence::CodeDependent, None))
                    }
                    // A repository-bound source is an ambiguous typed origin
                    // while the ownership reader is unconfigured: Unknown,
                    // never a negative lineage lookup (canonical 6a).
                    None if source.repository_key.is_some() => {
                        Ok((CandidateEvidence::Unknown, None))
                    }
                    Some(false) => Ok((CandidateEvidence::Ordinary, Some(record.provenance))),
                    None => {
                        // Ordinary source, ownership reader unconfigured: the
                        // chunk's own kind is ordinary, so the origin is not
                        // code-owned; continue through its dependencies.
                        Ok((CandidateEvidence::Ordinary, Some(record.provenance)))
                    }
                }
            }
            EvidenceTargetType::Entity => {
                let Some(id) = reference.target_id.as_deref() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Entity, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS || *work >= MAX_DEPENDENCY_REFERENCES {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_entity(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self
                    .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                    .await?
                {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                if entity_is_code_kind(&record) {
                    if let Some(collected) = code_dependencies.as_deref_mut() {
                        collected.push(id.to_owned());
                    }
                    return Ok((CandidateEvidence::CodeDependent, None));
                }
                match self.in_code_lineage(id, request).await? {
                    Some(true) => {
                        if let Some(collected) = code_dependencies.as_deref_mut() {
                            collected.push(id.to_owned());
                        }
                        Ok((CandidateEvidence::CodeDependent, None))
                    }
                    Some(false) => Ok((CandidateEvidence::Ordinary, Some(record.provenance))),
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
                if resolved.len() > MAX_DEPENDENCY_RECORDS || *work >= MAX_DEPENDENCY_REFERENCES {
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
            // Relationship references resolve canonically through the graph
            // port; the relationship itself is not a code-binding record, so
            // classification continues through its own provenance/evidence.
            EvidenceTargetType::Relationship => {
                let Some(id) = reference.target_id.as_deref() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&EvidenceTargetType::Relationship, id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS || *work >= MAX_DEPENDENCY_REFERENCES {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let Some(record) = self
                    .knowledge
                    .get_relationship(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !self
                    .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                    .await?
                {
                    return Ok((CandidateEvidence::Unknown, None));
                }
                let mut provenance = record.provenance.clone();
                provenance.evidence.extend(record.evidence.iter().cloned());
                Ok((CandidateEvidence::Ordinary, Some(provenance)))
            }
            // Derived-record categories: resolve the canonical record and
            // continue the walk through its own provenance. An ID-less
            // derived reference is unresolved evidence, never ordinary
            // (canonical 6a).
            EvidenceTargetType::Memory | EvidenceTargetType::Belief => {
                let Some(id) = reference.target_id.clone() else {
                    return Ok((CandidateEvidence::Unknown, None));
                };
                if !resolved.insert(target_key(&reference.target_type, &id)) {
                    return Ok((CandidateEvidence::Ordinary, None));
                }
                if resolved.len() > MAX_DEPENDENCY_RECORDS || *work >= MAX_DEPENDENCY_REFERENCES {
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
                        Ok((CandidateEvidence::Ordinary, Some(record.provenance)))
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
                        // Typed belief dependencies (canonical 6b): sources
                        // and synthesizer inputs join the walk.
                        let provenance = belief_dependency_provenance(&record);
                        Ok((CandidateEvidence::Ordinary, Some(provenance)))
                    }
                }
            }
            // Canonically unresolvable categories: documents have no scoped
            // canonical read on the knowledge port and concepts/events/urls
            // carry no canonical code association — unresolved evidence
            // degrades to Unknown, never Ordinary (canonical 6a).
            EvidenceTargetType::Document
            | EvidenceTargetType::Concept
            | EvidenceTargetType::Event
            | EvidenceTargetType::Url => Ok((CandidateEvidence::Unknown, None)),
        }
    }
}

/// Folds a canonical belief's TYPED dependency fields into one synthetic
/// provenance frame: `sources` (typed `BeliefSource` targets) and the
/// synthesizer derivation's `input_refs`, alongside the record's own
/// provenance evidence/derivations (canonical 6b).
fn belief_dependency_provenance(belief: &engram_domain::Belief) -> engram_domain::Provenance {
    let mut provenance = belief.provenance.clone();
    for source in &belief.sources {
        let target_type = match source.target_type {
            engram_domain::BeliefSourceTargetType::Memory => EvidenceTargetType::Memory,
            engram_domain::BeliefSourceTargetType::Chunk => EvidenceTargetType::Chunk,
            engram_domain::BeliefSourceTargetType::Entity => EvidenceTargetType::Entity,
            engram_domain::BeliefSourceTargetType::Relationship => EvidenceTargetType::Relationship,
            engram_domain::BeliefSourceTargetType::Belief => EvidenceTargetType::Belief,
            engram_domain::BeliefSourceTargetType::Assertion
            | engram_domain::BeliefSourceTargetType::Event
            | engram_domain::BeliefSourceTargetType::Concept => {
                // No canonical EvidenceRef equivalent: unresolvable.
                provenance.evidence.push(engram_domain::EvidenceRef {
                    target_type: EvidenceTargetType::Event,
                    target_id: None,
                    uri: None,
                    quote: None,
                    location: None,
                });
                continue;
            }
        };
        provenance.evidence.push(engram_domain::EvidenceRef {
            target_type,
            target_id: Some(source.target_id.clone()),
            uri: None,
            quote: None,
            location: None,
        });
    }
    if let Some(synthesizer) = &belief.synthesizer {
        provenance.derivations.push(synthesizer.clone());
    }
    provenance
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

/// Upgrade the aggregate classification monoid: code dependence dominates,
/// unresolved evidence degrades ordinary (never the other way round).
fn upgrade(current: CandidateEvidence, next: CandidateEvidence) -> CandidateEvidence {
    match (current, next) {
        // Unknown is fail-closed. A positive code dependency must not erase
        // an unresolved, denied, or over-bound sibling in the same walk.
        (CandidateEvidence::Unknown, _) | (_, CandidateEvidence::Unknown) => {
            CandidateEvidence::Unknown
        }
        (CandidateEvidence::CodeDependent, _) | (_, CandidateEvidence::CodeDependent) => {
            CandidateEvidence::CodeDependent
        }
        (CandidateEvidence::Ordinary, CandidateEvidence::Ordinary) => CandidateEvidence::Ordinary,
    }
}

#[async_trait]
impl CandidateEvidenceResolver for RepositoryCandidateEvidence {
    async fn evidence(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<CandidateEvidence> {
        Ok(self
            .classify_with_dependencies(candidate, request)
            .await?
            .evidence)
    }

    /// Full classification with code dependency ids (canonical 6c): recipes
    /// with a SUCCESSFUL view must still validate derived records' code
    /// dependencies against that view.
    async fn classify_with_dependencies(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<EvidenceClassification> {
        let mut resolved = BTreeSet::new();
        let mut work = 0usize;
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
                    let mut code_dependencies = Vec::new();
                    let evidence = self
                        .dependencies(
                            &record.provenance,
                            request,
                            &mut resolved,
                            &mut work,
                            Some(&mut code_dependencies),
                        )
                        .await?;
                    Ok(EvidenceClassification::complete(
                        evidence,
                        code_dependencies,
                    ))
                }
                Some(_) | None => Ok(EvidenceClassification::incomplete(
                    CandidateEvidence::Unknown,
                )),
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
                    // Typed belief dependencies join the walk (canonical 6b).
                    let provenance = belief_dependency_provenance(&record);
                    let mut code_dependencies = Vec::new();
                    let evidence = self
                        .dependencies(
                            &provenance,
                            request,
                            &mut resolved,
                            &mut work,
                            Some(&mut code_dependencies),
                        )
                        .await?;
                    Ok(EvidenceClassification::complete(
                        evidence,
                        code_dependencies,
                    ))
                }
                Some(_) | None => Ok(EvidenceClassification::incomplete(
                    CandidateEvidence::Unknown,
                )),
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
                        if code_chunk(record.kind) {
                            return Ok(EvidenceClassification::code_dependent(vec![
                                candidate.target_id.clone(),
                            ]));
                        }
                        match self
                            .in_code_lineage(candidate.target_id.as_str(), request)
                            .await?
                        {
                            Some(true) => {
                                return Ok(EvidenceClassification::code_dependent(vec![
                                    candidate.target_id.clone(),
                                ]));
                            }
                            None if source.repository_key.is_some() => {
                                return Ok(EvidenceClassification::incomplete(
                                    CandidateEvidence::Unknown,
                                ));
                            }
                            Some(false) | None => {
                                let mut code_dependencies = Vec::new();
                                let evidence = self
                                    .dependencies(
                                        &record.provenance,
                                        request,
                                        &mut resolved,
                                        &mut work,
                                        Some(&mut code_dependencies),
                                    )
                                    .await?;
                                Ok(EvidenceClassification::complete(
                                    evidence,
                                    code_dependencies,
                                ))
                            }
                        }
                    }
                    Some(_) | None => Ok(EvidenceClassification::incomplete(
                        CandidateEvidence::Unknown,
                    )),
                },
                None => Ok(EvidenceClassification::incomplete(
                    CandidateEvidence::Unknown,
                )),
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
                    if !self
                        .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                        .await?
                    {
                        return Ok(EvidenceClassification::incomplete(
                            CandidateEvidence::Unknown,
                        ));
                    }
                    if entity_is_code_kind(&record) {
                        return Ok(EvidenceClassification::code_dependent(vec![
                            candidate.target_id.clone(),
                        ]));
                    }
                    match self
                        .in_code_lineage(candidate.target_id.as_str(), request)
                        .await?
                    {
                        Some(true) => Ok(EvidenceClassification::code_dependent(vec![
                            candidate.target_id.clone(),
                        ])),
                        Some(false) => {
                            let mut code_dependencies = Vec::new();
                            let evidence = self
                                .dependencies(
                                    &record.provenance,
                                    request,
                                    &mut resolved,
                                    &mut work,
                                    Some(&mut code_dependencies),
                                )
                                .await?;
                            Ok(EvidenceClassification::complete(
                                evidence,
                                code_dependencies,
                            ))
                        }
                        None => Ok(EvidenceClassification::incomplete(
                            CandidateEvidence::Unknown,
                        )),
                    }
                }
                Some(_) | None => Ok(EvidenceClassification::incomplete(
                    CandidateEvidence::Unknown,
                )),
            },
            _ => Ok(EvidenceClassification::incomplete(
                CandidateEvidence::Unknown,
            )),
        }
    }
}
