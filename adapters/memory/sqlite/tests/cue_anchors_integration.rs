//! Integration tests for cue anchor extraction (write path) and cue-mode
//! retrieval (T2 + T3 end-to-end round-trip, per spec `memory-cue-anchors`).

use chrono::Utc;
use engram_domain::*;
use engram_memory::MemoryService;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

// ---------- helpers ----------------------------------------------------------

fn tenant() -> &'static str {
    "tenant-cue-test"
}

fn scope() -> Scope {
    Scope {
        tenant: tenant().to_owned(),
        subject: Some("subject-cue".to_owned()),
        workspace: Some("ws-cue".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("agent-cue"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: Vec::new(),
        permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn write_req(text: &str) -> WriteMemoryRequest {
    let now = Utc::now();
    WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: Vec::new(), // no pre-populated entities
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(),
        requester: requester(),
        provenance: Provenance {
            source: "cue-test".to_owned(),
            actor: actor(),
            observed_at: now,
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        },
        policy: Policy {
            visibility: Visibility::Private,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        },
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn write_req_with_entities(text: &str, entities: Vec<EntityRef>) -> WriteMemoryRequest {
    let mut req = write_req(text);
    req.content.entities = entities;
    req
}

fn cue_request(query: &str, cues: Vec<Cue>) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Cue],
        filters: None,
        cues,
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn entity_cue(value: &str, op: CueOperator) -> Cue {
    Cue {
        slot: "entity".to_owned(),
        value: serde_json::json!(value),
        operator: Some(op),
        weight: None,
    }
}

fn kind_cue(value: &str) -> Cue {
    Cue {
        slot: "kind".to_owned(),
        value: serde_json::json!(value),
        operator: Some(CueOperator::Equals),
        weight: None,
    }
}

// ---------- T2: write-path entity extraction ---------------------------------

#[test]
fn write_populates_entities_from_text() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    let resp =
        block_on(svc.write_memory(write_req("Alice Chen reviewed the Project Atlas proposal")))
            .expect("write");

    let entities = &resp.record.content.entities;
    let names: Vec<_> = entities.iter().filter_map(|e| e.name.as_deref()).collect();
    assert!(
        names.contains(&"Alice Chen"),
        "expected Alice Chen, got {names:?}"
    );
    assert!(
        names.contains(&"Project Atlas"),
        "expected Project Atlas, got {names:?}"
    );
    for e in entities {
        assert_eq!(
            e.kind.as_deref(),
            Some("unknown"),
            "extraction always yields unknown kind"
        );
    }
}

#[test]
fn write_preserves_caller_entities() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    let caller_entity = EntityRef {
        id: None,
        kind: Some("custom".to_owned()),
        name: Some("MyEntity".to_owned()),
        aliases: Vec::new(),
    };
    // "MyEntity" is a single capitalised token — extraction won't produce it.
    let resp = block_on(svc.write_memory(write_req_with_entities(
        "MyEntity launched today",
        vec![caller_entity.clone()],
    )))
    .expect("write");

    let entities = &resp.record.content.entities;
    let my_entity = entities
        .iter()
        .find(|e| e.name.as_deref() == Some("MyEntity"));
    assert!(my_entity.is_some(), "caller entity should be preserved");
    assert_eq!(
        my_entity.unwrap().kind.as_deref(),
        Some("custom"),
        "caller kind wins"
    );
}

#[test]
fn write_no_cap_runs_leaves_entities_empty() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    let resp = block_on(svc.write_memory(write_req("no capitalised runs here"))).expect("write");
    assert!(resp.record.content.entities.is_empty());
}

// ---------- T3 / T4: cue-mode retrieval -------------------------------------

#[test]
fn cue_retrieve_entity_equals() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Sarah Johnson is deploying next week"))).unwrap();

    let payload = block_on(svc.retrieve(cue_request(
        "Sarah Johnson",
        vec![entity_cue("Sarah Johnson", CueOperator::Equals)],
    )))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 1);
    let score = &payload.items[0].score;
    assert!(score.cue_match.is_some());
    assert!((score.cue_match.unwrap() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn cue_retrieve_entity_contains() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Helios Platform is ready"))).unwrap();

    let payload = block_on(svc.retrieve(cue_request(
        "Helios",
        vec![entity_cue("Helios", CueOperator::Contains)],
    )))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 1);
    assert!(payload.items[0].score.cue_match.is_some());
}

#[test]
fn cue_retrieve_kind_from_caller_entity() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    // Extraction always yields kind:"unknown"; kind:"person" must come from caller.
    let caller = EntityRef {
        id: None,
        kind: Some("person".to_owned()),
        name: Some("Alice Chen".to_owned()),
        aliases: Vec::new(),
    };
    block_on(svc.write_memory(write_req_with_entities("Alice Chen joined", vec![caller]))).unwrap();
    // Second record with no kind:person.
    block_on(svc.write_memory(write_req("Helios Platform deployed"))).unwrap();

    let payload = block_on(svc.retrieve(cue_request("person anchors", vec![kind_cue("person")])))
        .expect("retrieve");

    assert_eq!(
        payload.items.len(),
        1,
        "only record with kind:person returned"
    );
    assert!(payload.items[0].score.cue_match.is_some());
}

#[test]
fn cue_retrieve_no_match_empty_result() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Helios Platform launched"))).unwrap();

    let payload = block_on(svc.retrieve(cue_request(
        "nobody",
        vec![entity_cue("Nobody Known", CueOperator::Equals)],
    )))
    .expect("retrieve");

    assert!(payload.items.is_empty());
}

#[test]
fn cue_mode_does_not_leak_keyword_scoring() {
    // The query keyword-matches the record, but the cue does not — so with
    // modes:[Cue] only, the result must be empty. Fails if keyword runs during Cue mode.
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Sarah Johnson is deploying"))).unwrap();

    let payload = block_on(svc.retrieve(RetrievalRequest {
        query: "Sarah Johnson".to_owned(), // keyword would match this
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Cue],
        filters: None,
        cues: vec![entity_cue("Nobody Known", CueOperator::Equals)], // cue does not match
        limit: None,
        budget: None,
        include_explanations: Some(true),
    }))
    .expect("retrieve");

    // Must be empty: keyword is suppressed, cue doesn't match.
    assert!(
        payload.items.is_empty(),
        "keyword must not run when modes:[Cue] — got {} items",
        payload.items.len()
    );
}

#[test]
fn cue_only_result_has_relevance_none() {
    // Even when the query happens to keyword-match the text, relevance must be None
    // on a cue-only result because keyword mode is not active.
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Project Orion is on track"))).unwrap();

    let payload = block_on(svc.retrieve(cue_request(
        "Project Orion",                                        // would keyword-match
        vec![entity_cue("Project Orion", CueOperator::Equals)], // cue matches
    )))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 1);
    assert!(
        payload.items[0].score.relevance.is_none(),
        "relevance must be None in cue-only mode"
    );
}

#[test]
fn modes_semantic_returns_empty_from_sql_adapter() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Project Orion is on track"))).unwrap();

    let req = RetrievalRequest {
        query: "project orion".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Semantic], // not handled by this adapter
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    };
    let payload = block_on(svc.retrieve(req)).expect("retrieve");
    assert!(
        payload.items.is_empty(),
        "Semantic mode not served by SqlMemoryService"
    );
}

#[test]
fn modes_keyword_explicit_works_same_as_default() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Project Orion is on track"))).unwrap();

    let default_req = RetrievalRequest {
        query: "project orion".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: None,
        budget: None,
        include_explanations: None,
    };
    let explicit_keyword_req = RetrievalRequest {
        modes: vec![RetrievalMode::Keyword],
        ..default_req.clone()
    };

    let default_payload = block_on(svc.retrieve(default_req)).expect("default retrieve");
    let explicit_payload = block_on(svc.retrieve(explicit_keyword_req)).expect("explicit retrieve");
    assert_eq!(default_payload.items.len(), explicit_payload.items.len());
}

#[test]
fn cue_retrieve_budget_overflow_into_omitted() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    // Seed three memories, each with a matching entity.
    for name in &["Alpha Beta", "Gamma Delta", "Epsilon Zeta"] {
        block_on(svc.write_memory(write_req(&format!("{name} are here")))).unwrap();
    }

    let req = RetrievalRequest {
        query: "test".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Cue],
        filters: None,
        cues: vec![entity_cue("a", CueOperator::Contains)], // all names contain "a"
        limit: Some(2),
        budget: None,
        include_explanations: None,
    };
    let payload = block_on(svc.retrieve(req)).expect("retrieve");
    assert_eq!(payload.items.len(), 2);
    assert_eq!(payload.omitted.len(), 1);
    assert!(
        payload
            .omitted
            .iter()
            .any(|o| matches!(o.reason, OmittedReason::BudgetExceeded)),
        "overflow record should be BudgetExceeded"
    );
}

#[test]
fn cue_retrieve_matched_cues_in_explanation() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    block_on(svc.write_memory(write_req("Project Orion is ready"))).unwrap();

    let cue = entity_cue("Orion", CueOperator::Contains);
    let payload = block_on(svc.retrieve(RetrievalRequest {
        query: "Project Orion".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Cue],
        filters: None,
        cues: vec![cue],
        limit: None,
        budget: None,
        include_explanations: Some(true),
    }))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 1);
    let explanation = payload.items[0].explanation.as_ref().expect("explanation");
    assert_eq!(explanation.matched_cues.len(), 1);
    assert_eq!(explanation.matched_terms, Vec::<String>::new()); // cue-only, no keyword terms
}

#[test]
fn both_modes_distinct_records_each_carry_correct_score_fields() {
    let svc = SqlMemoryService::open_in_memory().expect("open");

    // Record A: text matches "Alice Chen" by keyword; extracted entity "Alice Chen"
    // does NOT match the "Helios Platform" cue.
    block_on(svc.write_memory(write_req("Alice Chen joined the team"))).unwrap();

    // Record B: no keyword match for "Alice Chen"; entity "Helios Platform" matches cue.
    let helios = EntityRef {
        id: None,
        kind: Some("unknown".to_owned()),
        name: Some("Helios Platform".to_owned()),
        aliases: Vec::new(),
    };
    block_on(svc.write_memory(write_req_with_entities("unrelated text", vec![helios]))).unwrap();

    let payload = block_on(svc.retrieve(RetrievalRequest {
        query: "Alice Chen".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword, RetrievalMode::Cue],
        filters: None,
        cues: vec![entity_cue("Helios Platform", CueOperator::Equals)],
        limit: None,
        budget: None,
        include_explanations: Some(true), // AC11 requires explanation on both-modes
    }))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 2);

    let kw_result = payload
        .items
        .iter()
        .find(|r| r.score.relevance.is_some() && r.score.cue_match.is_none());
    let cue_result = payload
        .items
        .iter()
        .find(|r| r.score.cue_match.is_some() && r.score.relevance.is_none());

    assert!(kw_result.is_some(), "keyword-only record should be present");
    assert!(cue_result.is_some(), "cue-only record should be present");

    // AC11: keyword-only record has matched_cues empty; cue-only has it non-empty.
    let kw = kw_result.unwrap();
    let cue = cue_result.unwrap();
    let kw_explanation = kw.explanation.as_ref().expect("kw explanation present");
    let cue_explanation = cue.explanation.as_ref().expect("cue explanation present");
    assert!(
        kw_explanation.matched_cues.is_empty(),
        "keyword-only matched_cues should be empty"
    );
    assert!(
        !cue_explanation.matched_cues.is_empty(),
        "cue-only matched_cues should be populated"
    );

    // fusion_trace source labels
    let kw_trace = kw.fusion_trace.as_ref().expect("kw fusion_trace");
    let cue_trace = cue.fusion_trace.as_ref().expect("cue fusion_trace");
    assert_eq!(kw_trace.source, "sql.memory.keyword");
    assert_eq!(cue_trace.source, "sql.memory.cue");
    assert!(
        kw_trace.rerank_score.is_some(),
        "keyword-only rerank_score set"
    );
    assert!(
        cue_trace.rerank_score.is_none(),
        "cue-only rerank_score is None"
    );
}

#[test]
fn both_modes_same_record_matched_by_both() {
    let svc = SqlMemoryService::open_in_memory().expect("open");
    // "Alice Chen" will be extracted as an entity and the text will keyword-match.
    block_on(svc.write_memory(write_req("Alice Chen joined the team"))).unwrap();

    let payload = block_on(svc.retrieve(RetrievalRequest {
        query: "Alice Chen".to_owned(),
        scope: scope(),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword, RetrievalMode::Cue],
        filters: None,
        cues: vec![entity_cue("Alice Chen", CueOperator::Equals)],
        limit: None,
        budget: None,
        include_explanations: Some(true),
    }))
    .expect("retrieve");

    assert_eq!(payload.items.len(), 1, "single record matched by both");
    let item = &payload.items[0];
    assert!(
        item.score.relevance.is_some(),
        "relevance set for keyword match"
    );
    assert!(
        item.score.cue_match.is_some(),
        "cue_match set for cue match"
    );
    let total = item.score.total;
    let rel = item.score.relevance.unwrap();
    let cm = item.score.cue_match.unwrap();
    assert!(
        (total - rel.max(cm)).abs() < f32::EPSILON,
        "total == max(relevance, cue_match)"
    );

    let trace = item.fusion_trace.as_ref().expect("fusion_trace");
    assert_eq!(trace.source, "sql.memory.keyword+cue");
    assert_eq!(trace.fusion_strategy, Some(FusionStrategy::MaxScore));
    assert!(
        trace.rerank_score.is_none(),
        "both-modes rerank_score is None"
    );
    assert!(
        trace.rerank_strategy.is_none(),
        "both-modes rerank_strategy is None"
    );
}

// ---------- T4: end-to-end smoke test ----------------------------------------

#[test]
fn e2e_write_and_retrieve_full_round_trip() {
    let svc = SqlMemoryService::open_in_memory().expect("open");

    let resp = block_on(svc.write_memory(write_req(
        "Sarah Johnson and the Helios Platform team are deploying next week",
    )))
    .expect("write");

    let entities = &resp.record.content.entities;
    let names: Vec<_> = entities.iter().filter_map(|e| e.name.as_deref()).collect();
    assert!(
        names.contains(&"Sarah Johnson"),
        "Sarah Johnson extracted: {names:?}"
    );
    assert!(
        names.contains(&"Helios Platform"),
        "Helios Platform extracted: {names:?}"
    );

    // Cue by entity name — Equals
    let by_name = block_on(svc.retrieve(cue_request(
        "Sarah Johnson",
        vec![entity_cue("Sarah Johnson", CueOperator::Equals)],
    )))
    .expect("retrieve by name");
    assert_eq!(by_name.items.len(), 1);

    // No match
    let no_match = block_on(svc.retrieve(cue_request(
        "nobody",
        vec![entity_cue("Nobody Known", CueOperator::Equals)],
    )))
    .expect("retrieve no match");
    assert!(no_match.items.is_empty());
}
