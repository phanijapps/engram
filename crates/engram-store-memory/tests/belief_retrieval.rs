use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{BeliefRepository, Clock, CoreResult, MemoryService, PolicyAuthorizer};
use engram_domain::*;
use engram_store_memory::{InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct AllowAll;

impl PolicyAuthorizer for AllowAll {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

#[test]
fn retrieve_returns_matching_active_belief_with_explanation() {
    let service = service();
    seed_belief(
        &service,
        belief(
            "belief-active",
            "Engram belief retrieval keeps derived stance distinct.",
        ),
    );

    let context = block_on(service.retrieve(retrieval_request(
        "belief retrieval stance",
        scope("engram"),
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    let item = &context.items[0];
    assert_eq!(item.target_type, RetrievalTargetType::Belief);
    assert_eq!(item.target_id, "belief-active");
    assert!(item.content.contains("derived stance"));
    assert_eq!(
        item.fusion_trace
            .as_ref()
            .map(|trace| trace.source.as_str()),
        Some("belief.keyword")
    );
    let explanation = item.explanation.as_ref().expect("belief explanation");
    assert_eq!(
        explanation.matched_terms,
        vec!["belief", "retrieval", "stance"]
    );
    assert_eq!(
        explanation.source_summary.as_deref(),
        Some("Belief retrieval fixture")
    );
}

#[test]
fn retrieve_skips_non_retrievable_beliefs_and_reports_policy_omissions() {
    let service = service();
    seed_belief(
        &service,
        belief("belief-active", "Engram belief retrieval target."),
    );
    seed_belief(
        &service,
        Belief {
            stale: Some(true),
            ..belief("belief-stale", "Engram belief retrieval target stale.")
        },
    );
    seed_belief(
        &service,
        Belief {
            status: BeliefStatus::Retracted,
            ..belief(
                "belief-retracted",
                "Engram belief retrieval target retracted.",
            )
        },
    );
    seed_belief(
        &service,
        Belief {
            valid_until: Some(past_time()),
            ..belief("belief-expired", "Engram belief retrieval target expired.")
        },
    );
    seed_belief(
        &service,
        Belief {
            policy: policy(vec![AllowedUse::TrainingExport], None),
            ..belief("belief-denied", "Engram belief retrieval target denied.")
        },
    );
    seed_belief(
        &service,
        Belief {
            confidence: 0.2,
            ..belief(
                "belief-low",
                "Engram belief retrieval target low confidence.",
            )
        },
    );
    seed_belief(
        &service,
        Belief {
            scope: scope("private"),
            ..belief("belief-private", "Engram belief retrieval target private.")
        },
    );

    let mut request = retrieval_request("belief retrieval target", scope("engram"));
    request.filters = Some(QueryFilter {
        memory_kinds: Vec::new(),
        source_kinds: Vec::new(),
        chunk_kinds: Vec::new(),
        concept_ids: Vec::new(),
        entity_ids: Vec::new(),
        since: Some(past_time()),
        until: Some(future_time()),
        min_confidence: Some(0.5),
        include_archived: Some(false),
    });
    let context = block_on(service.retrieve(request)).expect("retrieve context");

    assert_eq!(
        context
            .items
            .iter()
            .map(|item| item.target_id.as_str())
            .collect::<Vec<_>>(),
        vec!["belief-active"]
    );
    assert!(
        context
            .omitted
            .iter()
            .any(|omitted| omitted.target_id == "belief-expired"
                && omitted.reason == OmittedReason::Expired)
    );
    assert!(
        context
            .omitted
            .iter()
            .any(|omitted| omitted.target_id == "belief-denied"
                && omitted.reason == OmittedReason::PolicyDenied)
    );
    assert!(
        context
            .omitted
            .iter()
            .all(|omitted| omitted.target_id != "belief-private")
    );
}

#[test]
fn retrieve_truncates_after_memory_and_belief_candidates_are_fused() {
    let service = service();
    block_on(service.write_memory(write_request(
        "Engram retrieval combines memory candidates.",
        scope("engram"),
    )))
    .expect("write memory");
    seed_belief(
        &service,
        belief(
            "belief-combined",
            "Engram retrieval combines belief candidates.",
        ),
    );

    let mut request = retrieval_request("retrieval combines", scope("engram"));
    request.limit = Some(1);
    let context = block_on(service.retrieve(request)).expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::BudgetExceeded);
    assert!(
        context
            .omitted
            .iter()
            .any(|omitted| omitted.target_type == RetrievalTargetType::Memory
                || omitted.target_type == RetrievalTargetType::Belief)
    );
}

#[test]
fn open_contradiction_downranks_matching_belief_with_explanation() {
    let service = service();
    seed_belief(
        &service,
        belief(
            "belief-clear",
            "Engram contradiction ranking target remains clear.",
        ),
    );
    seed_belief(
        &service,
        belief(
            "belief-contradicted",
            "Engram contradiction ranking target remains reviewable.",
        ),
    );
    seed_contradiction(
        &service,
        contradiction(
            "contradiction-open",
            "belief-contradicted",
            scope("engram"),
            ContradictionStatus::Open,
        ),
    );

    let context = block_on(service.retrieve(retrieval_request(
        "contradiction ranking target",
        scope("engram"),
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 2);
    assert_eq!(context.items[0].target_id, "belief-clear");
    assert_eq!(context.items[1].target_id, "belief-contradicted");
    assert!(context.items[0].score.total > context.items[1].score.total);
    let explanation = context.items[1]
        .explanation
        .as_ref()
        .expect("contradicted belief explanation");
    assert!(explanation.reason.contains("open contradiction"));
    assert_eq!(
        explanation.source_summary.as_deref(),
        Some("Belief retrieval fixture; open contradictions: contradiction-open")
    );
}

#[test]
fn resolved_contradiction_does_not_downrank_matching_belief() {
    let service = service();
    seed_belief(
        &service,
        belief(
            "belief-resolved",
            "Engram contradiction ranking resolved target.",
        ),
    );
    seed_contradiction(
        &service,
        contradiction(
            "contradiction-resolved",
            "belief-resolved",
            scope("engram"),
            ContradictionStatus::Resolved,
        ),
    );

    let context = block_on(service.retrieve(retrieval_request(
        "contradiction ranking resolved target",
        scope("engram"),
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert!(context.items[0].score.total > 0.9);
    assert!(
        !context.items[0]
            .explanation
            .as_ref()
            .expect("belief explanation")
            .reason
            .contains("open contradiction")
    );
}

#[test]
fn out_of_scope_contradiction_does_not_downrank_matching_belief() {
    let service = service();
    seed_belief(
        &service,
        belief(
            "belief-scoped",
            "Engram contradiction ranking scoped target.",
        ),
    );
    seed_contradiction(
        &service,
        contradiction(
            "contradiction-private",
            "belief-scoped",
            scope("private"),
            ContradictionStatus::Open,
        ),
    );

    let context = block_on(service.retrieve(retrieval_request(
        "contradiction ranking scoped target",
        scope("engram"),
    )))
    .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert!(context.items[0].score.total > 0.9);
}

fn service() -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        Arc::new(AllowAll),
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

fn seed_belief(service: &InMemoryMemoryService, belief: Belief) {
    block_on(service.put_belief(belief)).expect("put belief");
}

fn seed_contradiction(service: &InMemoryMemoryService, contradiction: Contradiction) {
    block_on(service.put_contradiction(contradiction)).expect("put contradiction");
}

fn belief(id: &str, content: &str) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope("engram"),
        subject: BeliefSubject {
            key: "engram".to_owned(),
            entity_ref: Some(EntityRef {
                id: Some(Id::from("entity-engram")),
                kind: Some("project".to_owned()),
                name: Some("Engram".to_owned()),
                aliases: vec!["Agentic Memory".to_owned()],
            }),
            concept_ref: None,
            aliases: vec!["Engram".to_owned()],
        },
        content: content.to_owned(),
        status: BeliefStatus::Active,
        confidence: 0.9,
        sources: vec![BeliefSource {
            target_type: BeliefSourceTargetType::Memory,
            target_id: "memory-source".to_owned(),
            weight: Some(1.0),
            confidence: Some(0.9),
            valid_from: None,
            valid_until: None,
        }],
        valid_from: None,
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: Some("Belief retrieval fixture".to_owned()),
        embedding_refs: Vec::new(),
        policy: policy(vec![AllowedUse::Retrieval], None),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }
}

fn contradiction(
    id: &str,
    belief_id: &str,
    scope: Scope,
    status: ContradictionStatus,
) -> Contradiction {
    Contradiction {
        id: Id::from(id),
        scope,
        kind: ContradictionKind::Tension,
        targets: vec![ContradictionTarget {
            target_type: ContradictionTargetType::Belief,
            target_id: belief_id.to_owned(),
            role: Some("claim".to_owned()),
        }],
        severity: 0.7,
        status,
        reasoning: Some("contradiction-aware ranking fixture".to_owned()),
        detected_by: None,
        resolution: None,
        provenance: provenance(),
        detected_at: fixed_time(),
        updated_at: None,
    }
}

fn retrieval_request(query: &str, scope: Scope) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope,
        requester: requester(),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: Vec::new(),
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: None,
            include_archived: Some(false),
        }),
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn write_request(text: &str, scope: Scope) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope,
        requester: requester(),
        provenance: provenance(),
        policy: policy(vec![AllowedUse::Retrieval], None),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Belief Retrieval Agent".to_owned()),
        metadata: None,
    }
}

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some(workspace.to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy(allowed_uses: Vec<AllowedUse>, expires_at: Option<Timestamp>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses,
        expires_at,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "belief_retrieval_test".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.9),
        method: Some("test".to_owned()),
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn past_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("past timestamp")
}

fn future_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0)
        .single()
        .expect("future timestamp")
}
