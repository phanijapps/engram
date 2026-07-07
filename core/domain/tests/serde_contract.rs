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
