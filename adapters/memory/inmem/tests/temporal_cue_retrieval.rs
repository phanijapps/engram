//! Temporal + Cue retrieval dispatch.
//!
//! Pins `RetrievalMode::Temporal` and `RetrievalMode::Cue` in the in-memory
//! adapter: temporal recall orders by recency within the time window; cue recall
//! matches memories by slot/value cues against their links.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use engram_domain::*;
use engram_memory::{Clock, CoreError, CoreResult, MemoryService, PolicyAuthorizer};
use engram_store_memory::{InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;
use serde_json::Value;

/// Clock that advances one second on every `now()` call so written memories get
/// strictly-increasing `created_at` timestamps.
struct IncrementingClock(Mutex<DateTime<Utc>>);

impl IncrementingClock {
    fn new(start: DateTime<Utc>) -> Self {
        Self(Mutex::new(start))
    }
}

impl Clock for IncrementingClock {
    fn now(&self) -> DateTime<Utc> {
        let mut guard = self.0.lock().expect("clock lock");
        let current = *guard;
        *guard = current + Duration::seconds(1);
        current
    }
}

struct AllowAll;
impl PolicyAuthorizer for AllowAll {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
}

struct DenyRetrieve;
impl PolicyAuthorizer for DenyRetrieve {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "denied".to_owned(),
        })
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
}

fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-1"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
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

fn scope() -> Scope {
    Scope {
        tenant: "tenant-1".to_owned(),
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
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "temporal-cue-test".to_owned(),
        actor: actor(),
        observed_at: base_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn link(rel: &str, target_id: &str) -> MemoryLink {
    MemoryLink {
        rel: rel.to_owned(),
        target_type: LinkTargetType::Entity,
        target_id: target_id.to_owned(),
        provenance: None,
    }
}

fn write_with_links(text: &str, links: Vec<MemoryLink>) -> WriteMemoryRequest {
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
        scope: scope(),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links,
        idempotency_key: None,
    }
}

fn service(clock: Arc<dyn Clock>, authorizer: Arc<dyn PolicyAuthorizer>) -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        authorizer,
        clock,
        Arc::new(SequentialIdGenerator::new()),
    )
}

/// A retrieval request that isolates temporal/cue dispatch: a query that matches
/// no memory text, so keyword retrieval contributes nothing.
fn modes_request(
    modes: Vec<RetrievalMode>,
    cues: Vec<Cue>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> RetrievalRequest {
    RetrievalRequest {
        query: "zzznomatchzzz".to_owned(),
        scope: scope(),
        requester: requester(),
        modes,
        filters: Some(QueryFilter {
            memory_kinds: vec![MemoryKind::Fact],
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since,
            until,
            min_confidence: None,
            include_archived: Some(false),
        }),
        cues,
        limit: Some(50),
        budget: None,
        include_explanations: Some(true),
    }
}

fn cue(slot: &str, value: &str, op: CueOperator) -> Cue {
    Cue {
        slot: slot.to_owned(),
        value: Scalar::from(value.to_owned()),
        operator: Some(op),
        weight: None,
    }
}

#[test]
fn temporal_orders_newest_first_and_scores_recency() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links("alpha record", Vec::new()))).expect("write a");
    block_on(svc.write_memory(write_with_links("beta record", Vec::new()))).expect("write b");
    block_on(svc.write_memory(write_with_links("gamma record", Vec::new()))).expect("write c");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Temporal],
        Vec::new(),
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(
        context.items.len(),
        3,
        "all in-scope memories returned via temporal"
    );
    assert!(context.items[0].content.contains("gamma"), "newest first");
    assert!(context.items[1].content.contains("beta"));
    assert!(context.items[2].content.contains("alpha"));
    for item in &context.items {
        assert!(item.score.recency.is_some(), "recency sub-score populated");
    }
}

#[test]
fn temporal_window_includes_and_excludes() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    let written =
        block_on(svc.write_memory(write_with_links("solo record", Vec::new()))).expect("write");
    let created_at = written.record.created_at;

    let before = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Temporal],
        Vec::new(),
        None,
        Some(created_at - Duration::seconds(1)),
    )))
    .expect("retrieve before");
    assert!(
        before.items.is_empty(),
        "memory before the window is excluded"
    );

    let around = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Temporal],
        Vec::new(),
        Some(created_at - Duration::seconds(1)),
        Some(created_at + Duration::seconds(1)),
    )))
    .expect("retrieve around");
    assert_eq!(
        around.items.len(),
        1,
        "memory inside the window is included"
    );
    let windowed_recency = around.items[0].score.recency.expect("windowed recency");
    assert!(
        (0.0..=1.0).contains(&windowed_recency),
        "windowed recency within [0,1]"
    );
}

#[test]
fn cue_matches_link_equals() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links(
        "depends record",
        vec![link("depends_on", "svc-A")],
    )))
    .expect("write a");
    block_on(svc.write_memory(write_with_links(
        "unrelated",
        vec![link("depends_on", "svc-B")],
    )))
    .expect("write b");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue("depends_on", "svc-A", CueOperator::Equals)],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(context.items.len(), 1);
    assert!(
        context.items[0].score.cue_match.is_some(),
        "cue_match sub-score populated"
    );
    let explanation = context.items[0].explanation.as_ref().expect("explanation");
    assert_eq!(explanation.matched_cues.len(), 1);
    assert_eq!(explanation.matched_cues[0].slot, "depends_on");
}

#[test]
fn cue_contains_operator_matches() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links(
        "service record",
        vec![link("owns", "service-alpha-1")],
    )))
    .expect("write");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue("owns", "alpha", CueOperator::Contains)],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(context.items.len(), 1);
}

#[test]
fn cue_no_match_returns_nothing() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links(
        "record",
        vec![link("depends_on", "svc-B")],
    )))
    .expect("write");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue("depends_on", "svc-A", CueOperator::Equals)],
        None,
        None,
    )))
    .expect("retrieve");

    assert!(
        context.items.is_empty(),
        "non-matching cue yields no candidates"
    );
}

#[test]
fn temporal_respects_policy_denial() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(DenyRetrieve),
    );
    block_on(svc.write_memory(write_with_links("record", Vec::new()))).expect("write");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Temporal],
        Vec::new(),
        None,
        None,
    )))
    .expect("retrieve");

    assert!(context.items.is_empty());
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::PolicyDenied);
}

fn cue_value(slot: &str, value: Scalar, op: CueOperator) -> Cue {
    Cue {
        slot: slot.to_owned(),
        value,
        operator: Some(op),
        weight: None,
    }
}

#[test]
fn cue_starts_with_operator_matches() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links(
        "region record",
        vec![link("region", "eu-west-1")],
    )))
    .expect("write");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue_value(
            "region",
            Scalar::from("eu".to_owned()),
            CueOperator::StartsWith,
        )],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(context.items.len(), 1);
}

#[test]
fn cue_exists_operator_ignores_value() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links(
        "depends record",
        vec![link("depends_on", "svc-A")],
    )))
    .expect("write with slot");
    block_on(svc.write_memory(write_with_links("no link", Vec::new()))).expect("write no slot");

    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue_value("depends_on", Value::Null, CueOperator::Exists)],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(
        context.items.len(),
        1,
        "Exists matches the slot and ignores value"
    );
}

#[test]
fn cue_in_operator_matches_membership() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links("svc-a", vec![link("owns", "svc-A")])))
        .expect("write a");
    block_on(svc.write_memory(write_with_links("svc-c", vec![link("owns", "svc-C")])))
        .expect("write c");

    let members = Value::Array(vec![
        Value::from("svc-A".to_owned()),
        Value::from("svc-B".to_owned()),
    ]);
    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue_value("owns", members, CueOperator::In)],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(context.items.len(), 1, "only the member target_id matches");
}

#[test]
fn cue_range_operator_is_lexical() {
    let svc = service(
        Arc::new(IncrementingClock::new(base_time())),
        Arc::new(AllowAll),
    );
    block_on(svc.write_memory(write_with_links("grade f", vec![link("grade", "f")])))
        .expect("write f");
    block_on(svc.write_memory(write_with_links("grade z", vec![link("grade", "z")])))
        .expect("write z");

    let bounds = Value::Array(vec![
        Value::from("a".to_owned()),
        Value::from("m".to_owned()),
    ]);
    let context = block_on(svc.retrieve(modes_request(
        vec![RetrievalMode::Cue],
        vec![cue_value("grade", bounds, CueOperator::Range)],
        None,
        None,
    )))
    .expect("retrieve");

    assert_eq!(
        context.items.len(),
        1,
        "only f is within [a, m] lexicographically"
    );
}
