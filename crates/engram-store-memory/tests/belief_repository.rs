use chrono::{TimeZone, Utc};
use engram_core::{BeliefRepository, MemoryService};
use engram_domain::*;
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn resolved_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 30, 13, 0, 0)
        .single()
        .expect("resolved timestamp")
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn other_scope() -> Scope {
    Scope {
        tenant: "tenant-other".to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-belief"),
        kind: ActorKind::Agent,
        display_name: Some("Belief Test".to_owned()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "belief_repository_test".to_owned(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: vec![EvidenceRef {
            target_type: EvidenceTargetType::Chunk,
            target_id: Some("chunk-1".to_owned()),
            uri: None,
            quote: Some("Engram keeps beliefs derived from evidence.".to_owned()),
            location: None,
        }],
        derivations: Vec::new(),
        confidence: Some(0.8),
        method: Some("test".to_owned()),
    }
}

fn belief(id: &str) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope(),
        subject: BeliefSubject {
            key: "engram.boundaries".to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: "Engram separates beliefs from source truth.".to_owned(),
        status: BeliefStatus::Active,
        confidence: 0.8,
        sources: vec![BeliefSource {
            target_type: BeliefSourceTargetType::Chunk,
            target_id: "chunk-1".to_owned(),
            weight: Some(1.0),
            confidence: Some(0.8),
            valid_from: None,
            valid_until: None,
        }],
        valid_from: None,
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: Some("single source test belief".to_owned()),
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: fixed_time(),
        updated_at: None,
        metadata: None,
    }
}

fn contradiction(id: &str) -> Contradiction {
    Contradiction {
        id: Id::from(id),
        scope: scope(),
        kind: ContradictionKind::Tension,
        targets: vec![
            ContradictionTarget {
                target_type: ContradictionTargetType::Belief,
                target_id: "belief-1".to_owned(),
                role: Some("claim".to_owned()),
            },
            ContradictionTarget {
                target_type: ContradictionTargetType::Chunk,
                target_id: "chunk-2".to_owned(),
                role: Some("counterevidence".to_owned()),
            },
        ],
        severity: 0.6,
        status: ContradictionStatus::Open,
        reasoning: Some("test contradiction".to_owned()),
        detected_by: None,
        resolution: None,
        provenance: provenance(),
        detected_at: fixed_time(),
        updated_at: None,
    }
}

fn resolution(kind: ContradictionResolutionKind) -> ContradictionResolution {
    ContradictionResolution {
        kind,
        winning_target_id: Some("belief-1".to_owned()),
        actor: actor(),
        reason: Some("reviewed by maintainer".to_owned()),
        resolved_at: resolved_time(),
    }
}

fn retrieval_request(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: vec!["maintainer".to_owned()],
            permissions: vec!["memory.retrieve".to_owned()],
            on_behalf_of: None,
        },
        modes: vec![RetrievalMode::Keyword],
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

#[test]
fn belief_repository_accepts_evidence_linked_belief() {
    let service = InMemoryMemoryService::new();
    let belief = belief("belief-1");

    let stored = block_on(service.put_belief(belief.clone())).expect("put belief");

    assert_eq!(stored, belief);
    assert_eq!(stored.sources[0].target_id, "chunk-1");
}

#[test]
fn belief_repository_accepts_reviewable_contradiction() {
    let service = InMemoryMemoryService::new();
    let contradiction = contradiction("contradiction-1");

    let stored =
        block_on(service.put_contradiction(contradiction.clone())).expect("put contradiction");

    assert_eq!(stored, contradiction);
    assert_eq!(stored.targets.len(), 2);
    assert_eq!(stored.status, ContradictionStatus::Open);
}

#[test]
fn contradiction_lookup_respects_scope() {
    let service = InMemoryMemoryService::new();
    let contradiction = contradiction("contradiction-1");
    block_on(service.put_contradiction(contradiction.clone())).expect("put contradiction");

    let visible = block_on(service.get_contradiction(&contradiction.id, &scope()))
        .expect("get contradiction");
    let hidden = block_on(service.get_contradiction(&contradiction.id, &other_scope()))
        .expect("get contradiction outside scope");

    assert_eq!(visible, Some(contradiction));
    assert!(hidden.is_none());
}

#[test]
fn contradiction_resolution_updates_review_record_only() {
    let service = InMemoryMemoryService::new();
    let belief = belief("belief-1");
    let contradiction = contradiction("contradiction-1");
    let resolution = resolution(ContradictionResolutionKind::TargetWon);
    block_on(service.put_belief(belief.clone())).expect("put belief");
    block_on(service.put_contradiction(contradiction.clone())).expect("put contradiction");

    let resolved =
        block_on(service.resolve_contradiction(&contradiction.id, &scope(), resolution.clone()))
            .expect("resolve contradiction");

    assert_eq!(resolved.status, ContradictionStatus::Resolved);
    assert_eq!(resolved.resolution, Some(resolution));
    assert_eq!(resolved.updated_at, Some(resolved_time()));
    assert_eq!(resolved.targets, contradiction.targets);
    assert_eq!(resolved.provenance, contradiction.provenance);
    assert_eq!(resolved.detected_at, contradiction.detected_at);

    let context = block_on(service.retrieve(retrieval_request("source truth")))
        .expect("belief retrieval after resolution");
    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Belief);
    assert_eq!(context.items[0].target_id, belief.id.to_string());
}

#[test]
fn contradiction_resolution_maps_review_outcomes_to_status() {
    let service = InMemoryMemoryService::new();
    let ignored = contradiction("contradiction-ignored");
    let needs_more_evidence = contradiction("contradiction-open");
    block_on(service.put_contradiction(ignored.clone())).expect("put ignored contradiction");
    block_on(service.put_contradiction(needs_more_evidence.clone()))
        .expect("put open contradiction");

    let ignored = block_on(service.resolve_contradiction(
        &ignored.id,
        &scope(),
        resolution(ContradictionResolutionKind::ManualIgnore),
    ))
    .expect("ignore contradiction");
    let still_open = block_on(service.resolve_contradiction(
        &needs_more_evidence.id,
        &scope(),
        resolution(ContradictionResolutionKind::NeedsMoreEvidence),
    ))
    .expect("mark contradiction as needing evidence");

    assert_eq!(ignored.status, ContradictionStatus::Ignored);
    assert_eq!(still_open.status, ContradictionStatus::Open);
    assert!(still_open.resolution.is_some());
}

#[test]
fn contradiction_resolution_rejects_cross_scope_update() {
    let service = InMemoryMemoryService::new();
    let contradiction = contradiction("contradiction-1");
    block_on(service.put_contradiction(contradiction.clone())).expect("put contradiction");

    let error = block_on(service.resolve_contradiction(
        &contradiction.id,
        &other_scope(),
        resolution(ContradictionResolutionKind::TargetWon),
    ))
    .expect_err("cross-scope resolution should fail");
    let unchanged = block_on(service.get_contradiction(&contradiction.id, &scope()))
        .expect("get contradiction")
        .expect("contradiction remains");

    assert!(matches!(error, engram_core::CoreError::NotFound { .. }));
    assert_eq!(unchanged.status, ContradictionStatus::Open);
    assert!(unchanged.resolution.is_none());
}
