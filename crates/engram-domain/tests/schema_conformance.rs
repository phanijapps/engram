use std::{fs, path::PathBuf};

use chrono::Utc;
use engram_domain::*;
use serde_json::{Value, json};

fn schema() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/v1/schemas/engram-v1.schema.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read v1 schema"))
        .expect("parse v1 schema")
}

fn definition_schema(definition: &str) -> Value {
    let base = schema();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": format!("#/$defs/{definition}"),
        "$defs": base["$defs"].clone()
    })
}

fn assert_matches_definition(definition: &str, value: Value) {
    let schema = definition_schema(definition);
    if let Err(error) = jsonschema::draft202012::validate(&schema, &value) {
        panic!("{definition} did not match accepted v1 schema: {error}\n{value:#}");
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Contract Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.write".to_owned()],
        on_behalf_of: None,
    }
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

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "schema_conformance_test".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn content() -> MemoryContent {
    MemoryContent {
        text: "Engram v1 uses accepted contracts before implementation.".to_owned(),
        summary: Some("Contract-first memory".to_owned()),
        entities: Vec::new(),
        language: Some("en".to_owned()),
        format: Some(MemoryContentFormat::Text),
        structured: None,
        hash: Some("sha256:schema-conformance".to_owned()),
    }
}

#[test]
fn rust_write_memory_request_matches_v1_schema() {
    let request = WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: content(),
        scope: scope(),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: Some("schema-write-001".to_owned()),
    };

    assert_matches_definition(
        "WriteMemoryRequest",
        serde_json::to_value(request).expect("serialize write request"),
    );
}

#[test]
fn rust_write_memory_response_matches_v1_schema() {
    let now = Utc::now();
    let record = MemoryRecord {
        id: Id::from("memory-001"),
        kind: MemoryKind::Fact,
        content: content(),
        scope: scope(),
        provenance: provenance(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        created_at: now,
        updated_at: None,
        metadata: None,
    };
    let event = MemoryEvent {
        id: Id::from("event-001"),
        kind: MemoryEventKind::Written,
        scope: scope(),
        actor: actor(),
        memory_id: Some(Id::from("memory-001")),
        payload: json!({"idempotencyKey": "schema-write-001"}),
        provenance: provenance(),
        occurred_at: now,
        recorded_at: now,
    };
    let response = WriteMemoryResponse {
        record,
        event,
        deduplicated: Some(false),
    };

    assert_matches_definition(
        "WriteMemoryResponse",
        serde_json::to_value(response).expect("serialize write response"),
    );
}

#[test]
fn rust_retrieval_request_matches_v1_schema() {
    let request = RetrievalRequest {
        query: "What does Engram use as its contract source?".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: vec![MemoryKind::Fact],
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: Some(0.5),
            include_archived: Some(false),
        }),
        cues: Vec::new(),
        limit: Some(5),
        budget: Some(ContextBudget {
            max_items: Some(3),
            max_tokens: Some(1200),
            max_bytes: None,
        }),
        include_explanations: Some(true),
    };

    assert_matches_definition(
        "RetrievalRequest",
        serde_json::to_value(request).expect("serialize retrieval request"),
    );
}
