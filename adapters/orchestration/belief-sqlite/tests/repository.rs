use chrono::Utc;
use engram_core::{BeliefRepository, ContradictionDetector};
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
