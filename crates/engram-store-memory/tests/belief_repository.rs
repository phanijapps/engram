use engram_core::BeliefRepository;
use engram_domain::*;
use engram_store_memory::InMemoryMemoryService;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-demo".to_owned(),
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

#[test]
fn belief_repository_accepts_evidence_linked_belief() {
    let service = InMemoryMemoryService::new();
    let belief = Belief {
        id: Id::from("belief-1"),
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
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    };

    let stored = block_on(service.put_belief(belief.clone())).expect("put belief");

    assert_eq!(stored, belief);
    assert_eq!(stored.sources[0].target_id, "chunk-1");
}

#[test]
fn belief_repository_accepts_reviewable_contradiction() {
    let service = InMemoryMemoryService::new();
    let contradiction = Contradiction {
        id: Id::from("contradiction-1"),
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
        detected_at: chrono::Utc::now(),
        updated_at: None,
    };

    let stored =
        block_on(service.put_contradiction(contradiction.clone())).expect("put contradiction");

    assert_eq!(stored, contradiction);
    assert_eq!(stored.targets.len(), 2);
    assert_eq!(stored.status, ContradictionStatus::Open);
}
