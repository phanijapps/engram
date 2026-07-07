use chrono::Utc;
use engram_domain::*;

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-1"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scope(session: Option<&str>) -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: session.map(str::to_owned),
        environment: Some("test".to_owned()),
    }
}

fn policy(retention: Retention) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance(now: Timestamp) -> Provenance {
    Provenance {
        source: "memory_roles_test".to_owned(),
        actor: actor(),
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn record(kind: MemoryKind, retention: Retention, session: Option<&str>) -> MemoryRecord {
    let now = Utc::now();
    MemoryRecord {
        id: Id::from("memory-1"),
        kind,
        content: MemoryContent {
            text: "memory role fixture".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope(session),
        provenance: provenance(now),
        policy: policy(retention),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: now,
        updated_at: None,
        metadata: None,
    }
}

#[test]
fn session_bounded_observation_is_working_memory_trace() {
    let memory = record(
        MemoryKind::Observation,
        Retention::Session,
        Some("session-1"),
    );

    assert_eq!(memory.role(), MemoryRole::Working);
}

#[test]
fn durable_episode_defaults_to_episodic_memory() {
    let memory = record(MemoryKind::Episode, Retention::Durable, Some("session-1"));

    assert_eq!(memory.role(), MemoryRole::Episodic);
}

#[test]
fn fact_preference_relationship_and_artifact_are_semantic_by_default() {
    for kind in [
        MemoryKind::Fact,
        MemoryKind::Preference,
        MemoryKind::Relationship,
        MemoryKind::Artifact,
    ] {
        let memory = record(kind, Retention::Durable, None);
        assert_eq!(memory.role(), MemoryRole::Semantic);
    }
}

#[test]
fn procedure_defaults_to_procedural_memory() {
    let memory = record(MemoryKind::Procedure, Retention::Durable, None);

    assert_eq!(memory.role(), MemoryRole::Procedural);
}

#[test]
fn derived_role_is_not_a_memory_record_wire_field() {
    let memory = record(MemoryKind::Fact, Retention::Durable, None);

    let value = serde_json::to_value(memory).expect("serialize memory record");

    assert!(value.get("role").is_none());
}
