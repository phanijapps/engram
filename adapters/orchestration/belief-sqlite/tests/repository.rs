use chrono::{TimeZone, Utc};
use engram_core::{BeliefQuery, BeliefReferenceQuery, BeliefRepository, ContradictionDetector};
use engram_domain::*;
use engram_store_belief_sqlite::SqlBeliefStore;
use futures::executor::block_on;

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "belief-sqlite-test".to_owned(),
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: Some("Belief Agent".to_owned()),
            metadata: None,
        },
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn ts(seconds: i64) -> Timestamp {
    Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
}

fn belief(id: &str, tenant: &str, key: &str, content: &str, confidence: f32) -> Belief {
    Belief {
        id: Id::from(id),
        scope: scope(tenant),
        subject: BeliefSubject {
            key: key.to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: content.to_owned(),
        status: BeliefStatus::Active,
        confidence,
        sources: Vec::new(),
        valid_from: None,
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: None,
        reasoning: None,
        embedding_refs: Vec::new(),
        policy: policy(),
        provenance: provenance(),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

#[test]
fn get_belief_uses_valid_time_and_rejects_record_time_history() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let mut old = belief("belief-old", "tenant-a", "svc-a", "old", 0.7);
    old.valid_from = Some(ts(10));
    old.valid_until = Some(ts(20));
    old.created_at = ts(10);
    let mut current = belief("belief-current", "tenant-a", "svc-a", "current", 0.9);
    current.valid_from = Some(ts(20));
    current.created_at = ts(20);
    let mut stale = belief("belief-stale", "tenant-a", "svc-a", "stale", 0.8);
    stale.valid_from = Some(ts(30));
    stale.status = BeliefStatus::Stale;
    stale.stale = Some(true);
    stale.created_at = ts(30);

    block_on(store.put_belief(old)).expect("put");
    block_on(store.put_belief(current)).expect("put");
    block_on(store.put_belief(stale)).expect("put");

    let during_old = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(15),
    )))
    .expect("get")
    .expect("belief");
    assert_eq!(during_old.id, Id::from("belief-old"));

    let during_current = block_on(store.get_belief(BeliefQuery::live_subject(
        scope("tenant-a"),
        "svc-a",
        ts(40),
    )))
    .expect("get")
    .expect("belief");
    assert_eq!(during_current.id, Id::from("belief-current"));

    let mut record_time_query = BeliefQuery::live_subject(scope("tenant-a"), "svc-a", ts(40));
    record_time_query.recorded_at = Some(ts(25));
    assert!(block_on(store.get_belief(record_time_query)).is_err());
}

#[test]
fn upsert_belief_is_idempotent_by_scope_subject_and_valid_from() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let mut first = belief("belief-original", "tenant-a", "svc-a", "old", 0.7);
    first.valid_from = Some(ts(10));
    let mut replacement_payload = belief("belief-new-id", "tenant-a", "svc-a", "new", 0.8);
    replacement_payload.valid_from = Some(ts(10));

    let stored = block_on(store.upsert_belief(first)).expect("upsert");
    assert_eq!(stored.id, Id::from("belief-original"));
    let updated = block_on(store.upsert_belief(replacement_payload)).expect("upsert");
    assert_eq!(updated.id, Id::from("belief-original"));
    assert_eq!(updated.content, "new");

    let visible = block_on(store.list_beliefs(&scope("tenant-a"))).expect("list");
    assert_eq!(visible.len(), 1);
}

#[test]
fn lifecycle_methods_and_source_reference_queries_match_live_beliefs() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let mut first = belief("belief-1", "tenant-a", "svc-a", "up", 0.7);
    first.valid_from = Some(ts(10));
    first.sources = vec![BeliefSource {
        target_type: BeliefSourceTargetType::Memory,
        target_id: "fact-1".to_owned(),
        weight: None,
        confidence: None,
        valid_from: None,
        valid_until: None,
    }];
    block_on(store.put_belief(first)).expect("put");

    let referenced = block_on(store.beliefs_referencing_source(BeliefReferenceQuery {
        scope: scope("tenant-a"),
        source_type: BeliefSourceTargetType::Memory,
        source_id: "fact-1".to_owned(),
        valid_at: Some(ts(20)),
    }))
    .expect("references");
    assert_eq!(referenced.len(), 1);

    let stale = block_on(store.mark_stale(&Id::from("belief-1"), &scope("tenant-a"), ts(21)))
        .expect("mark stale");
    assert_eq!(stale.status, BeliefStatus::Stale);
    assert_eq!(
        block_on(store.list_stale(&scope("tenant-a")))
            .expect("stale")
            .len(),
        1
    );
    assert!(
        block_on(store.beliefs_referencing_source(BeliefReferenceQuery::new(
            scope("tenant-a"),
            BeliefSourceTargetType::Memory,
            "fact-1"
        )))
        .expect("references")
        .is_empty()
    );

    let active = block_on(store.clear_stale(&Id::from("belief-1"), &scope("tenant-a"), ts(22)))
        .expect("clear stale");
    assert_eq!(active.status, BeliefStatus::Active);

    let superseded = block_on(store.supersede_belief(
        &Id::from("belief-1"),
        &scope("tenant-a"),
        Id::from("belief-2"),
        ts(30),
    ))
    .expect("supersede");
    assert_eq!(superseded.status, BeliefStatus::Superseded);
    assert_eq!(superseded.valid_until, Some(ts(30)));
    assert_eq!(superseded.superseded_by, Some(Id::from("belief-2")));

    let retracted =
        block_on(store.retract_belief(&Id::from("belief-1"), &scope("tenant-a"), ts(31)))
            .expect("retract");
    assert_eq!(retracted.status, BeliefStatus::Retracted);
    assert_eq!(retracted.valid_until, Some(ts(31)));
    assert_eq!(retracted.superseded_by, None);
}

#[test]
fn beliefs_round_trip_and_list_scoped() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    block_on(store.put_belief(belief("belief-1", "tenant-a", "svc-a", "up", 0.9))).expect("put");
    block_on(store.put_belief(belief("belief-2", "tenant-a", "svc-a", "healthy", 0.8)))
        .expect("put");
    block_on(store.put_belief(belief("belief-3", "tenant-b", "svc-a", "up", 0.9))).expect("put");
    // Bi-temporal valid_from/valid_until round-trip through the JSON payload.
    let mut timed = belief("belief-timed", "tenant-a", "svc-b", "v1", 0.5);
    timed.valid_from = Some(Utc::now());
    timed.valid_until = Some(Utc::now());
    block_on(store.put_belief(timed)).expect("put timed");

    let visible = block_on(store.list_beliefs(&scope("tenant-a"))).expect("list");
    let hidden = block_on(store.list_beliefs(&scope("tenant-b"))).expect("list");
    let reloaded = visible
        .iter()
        .find(|b| b.id == Id::from("belief-timed"))
        .expect("timed belief present");
    assert!(reloaded.valid_from.is_some());
    assert!(reloaded.valid_until.is_some());

    assert_eq!(visible.len(), 3); // belief-1, belief-2, belief-timed
    assert_eq!(hidden.len(), 1); // belief-3
}

#[test]
fn contradiction_get_and_resolve_scoped() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let contradiction = Contradiction {
        id: Id::from("contradiction-1"),
        scope: scope("tenant-a"),
        kind: ContradictionKind::Logical,
        targets: vec![ContradictionTarget {
            target_type: ContradictionTargetType::Belief,
            target_id: "belief-1".to_owned(),
            role: None,
        }],
        severity: 0.8,
        status: ContradictionStatus::Open,
        reasoning: Some("disagree".to_owned()),
        detected_by: None,
        resolution: None,
        provenance: provenance(),
        detected_at: Utc::now(),
        updated_at: None,
    };
    block_on(store.put_contradiction(contradiction)).expect("put contradiction");

    let visible =
        block_on(store.get_contradiction(&Id::from("contradiction-1"), &scope("tenant-a")))
            .expect("get");
    let hidden =
        block_on(store.get_contradiction(&Id::from("contradiction-1"), &scope("tenant-b")))
            .expect("get hidden");
    assert!(visible.is_some());
    assert!(hidden.is_none());

    let resolution = ContradictionResolution {
        kind: ContradictionResolutionKind::ManualIgnore,
        winning_target_id: None,
        actor: Actor {
            id: Id::from("agent-1"),
            kind: ActorKind::Agent,
            display_name: Some("Belief Agent".to_owned()),
            metadata: None,
        },
        reason: Some("not enough evidence".to_owned()),
        resolved_at: Utc::now(),
    };
    let resolved = block_on(store.resolve_contradiction(
        &Id::from("contradiction-1"),
        &scope("tenant-a"),
        resolution,
    ))
    .expect("resolve");
    assert_eq!(resolved.status, ContradictionStatus::Ignored);
    assert!(resolved.resolution.is_some());

    // A scope-hidden caller cannot resolve (NotFound).
    let err = block_on(store.resolve_contradiction(
        &Id::from("contradiction-1"),
        &scope("tenant-b"),
        ContradictionResolution {
            kind: ContradictionResolutionKind::ManualIgnore,
            winning_target_id: None,
            actor: Actor {
                id: Id::from("agent-1"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            reason: None,
            resolved_at: Utc::now(),
        },
    ));
    assert!(err.is_err());
}

#[test]
fn contradiction_insert_canonicalizes_pair_and_is_idempotent() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let first = Contradiction {
        id: Id::from("contradiction-first"),
        scope: scope("tenant-a"),
        kind: ContradictionKind::Logical,
        targets: vec![
            ContradictionTarget {
                target_type: ContradictionTargetType::Memory,
                target_id: "memory-1".to_owned(),
                role: None,
            },
            ContradictionTarget {
                target_type: ContradictionTargetType::Belief,
                target_id: "belief-1".to_owned(),
                role: None,
            },
        ],
        severity: 0.8,
        status: ContradictionStatus::Open,
        reasoning: None,
        detected_by: None,
        resolution: None,
        provenance: provenance(),
        detected_at: Utc::now(),
        updated_at: None,
    };
    let mut reversed = first.clone();
    reversed.id = Id::from("contradiction-second");
    reversed.targets.reverse();

    let stored = block_on(store.put_contradiction(first)).expect("put");
    assert_eq!(
        stored.targets[0].target_type,
        ContradictionTargetType::Belief
    );
    assert_eq!(
        stored.targets[1].target_type,
        ContradictionTargetType::Memory
    );

    let duplicate = block_on(store.put_contradiction(reversed)).expect("put duplicate");
    assert_eq!(duplicate.id, Id::from("contradiction-first"));
    assert_eq!(
        block_on(store.list_contradictions(&scope("tenant-a")))
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn detect_flags_disagreeing_active_beliefs_only() {
    let store = SqlBeliefStore::open_in_memory().expect("open store");
    let beliefs = vec![
        belief("b1", "tenant-a", "svc-a", "up", 0.9),
        belief("b2", "tenant-a", "svc-a", "down", 0.7), // disagrees with b1
        belief("b3", "tenant-a", "svc-b", "ok", 0.6),   // single-belief subject → no finding
        belief("b4", "tenant-a", "svc-c", "same", 0.5),
        belief("b5", "tenant-a", "svc-c", "same", 0.5), // uniform content → no finding
    ];
    let findings = block_on(store.detect_contradictions(&beliefs)).expect("detect");
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.kind, ContradictionKind::Logical);
    assert_eq!(finding.status, ContradictionStatus::Open);
    assert_eq!(finding.severity, 0.9); // max confidence of the disagreeing group
    assert_eq!(finding.targets.len(), 2);
    // A stale belief on the same subject is ignored.
    let mut stale = belief("b6", "tenant-a", "svc-a", "sideways", 0.4);
    stale.status = BeliefStatus::Stale;
    let findings_with_stale =
        block_on(store.detect_contradictions(&[beliefs[0].clone(), beliefs[1].clone(), stale]))
            .expect("detect");
    assert_eq!(findings_with_stale.len(), 1);
    assert_eq!(findings_with_stale[0].targets.len(), 2); // stale belief not targeted
}
