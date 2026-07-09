use chrono::Utc;
use engram_domain::*;
use serde_json::json;

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-1"),
        kind: ActorKind::Agent,
        display_name: Some("agent".to_owned()),
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
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

fn provenance(now: Timestamp) -> Provenance {
    Provenance {
        source: "test".to_owned(),
        actor: actor(),
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(0.9),
        method: Some("manual".to_owned()),
    }
}

#[test]
fn memory_record_uses_contract_json_names() {
    let now = Utc::now();
    let record = MemoryRecord {
        id: Id::from("mem-1"),
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: "Rust owns deterministic behavior.".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: Some("sha256:test".to_owned()),
        },
        scope: scope(),
        provenance: provenance(now),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };

    let value = serde_json::to_value(record).expect("serialize memory record");

    assert_eq!(value["createdAt"], json!(now));
    assert_eq!(value["content"]["hash"], json!("sha256:test"));
    assert_eq!(value["kind"], json!("fact"));
    assert_eq!(
        value["policy"]["allowedUses"],
        json!(["retrieval", "evaluation"])
    );
    assert!(value.get("created_at").is_none());
    assert!(value["content"].get("structured").is_none());
}

#[test]
fn knowledge_entity_serializes_bi_temporal_validity() {
    // ADR-0019: KnowledgeEntity carries optional validFrom/validUntil.
    let now = Utc::now();
    let entity = KnowledgeEntity {
        id: Id::from("entity-1"),
        graph_id: None,
        kind: EntityKind::Function,
        name: "parse".to_owned(),
        aliases: Vec::new(),
        scope: scope(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(now),
        created_at: now,
        updated_at: None,
        valid_from: Some(now),
        valid_until: Some(now),
        metadata: None,
    };

    let value = serde_json::to_value(&entity).expect("serialize entity");
    assert_eq!(value["validFrom"], json!(now));
    assert_eq!(value["validUntil"], json!(now));
    assert!(
        value.get("valid_from").is_none(),
        "camelCase contract names only"
    );

    // Round-trips losslessly.
    let back: KnowledgeEntity = serde_json::from_value(value).expect("deserialize entity");
    assert_eq!(back, entity);

    // None omits the keys (skip_serializing_if = Option::is_none).
    let mut omitted = entity;
    omitted.valid_from = None;
    omitted.valid_until = None;
    let omitted_value = serde_json::to_value(&omitted).expect("serialize");
    assert!(omitted_value.get("validFrom").is_none());
    assert!(omitted_value.get("validUntil").is_none());
}

#[test]
fn entity_kind_new_symbol_kinds_serialize_snake_case() {
    // ADR-0020: code-structural symbol kinds round-trip as snake_case.
    for (kind, expected) in [
        (EntityKind::Struct, "struct"),
        (EntityKind::Interface, "interface"),
        (EntityKind::Trait, "trait"),
        (EntityKind::TypeAlias, "type_alias"),
        (EntityKind::Enum, "enum"),
        (EntityKind::Endpoint, "endpoint"),
    ] {
        let value = serde_json::to_value(&kind).expect("serialize kind");
        assert_eq!(value, json!(expected));
        let back: EntityKind = serde_json::from_value(value).expect("deserialize kind");
        assert_eq!(back, kind);
    }
}
