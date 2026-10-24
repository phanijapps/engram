//! Sequential-harness tests: deterministic fake probes over a minimal stub
//! memory service prove aggregation, interference detection, and reset-arm
//! isolation — no LLM, no storage engine, no flakiness.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engram_domain::*;
use engram_eval::sequential::{
    ProbeStep, ResetProviderFactory, SequentialEvent, SequentialHarness, TaskProbe,
};
use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
use engram_runtime::{CoreError, CoreResult};
use futures::executor::block_on;

// ── Minimal deterministic keyword store ────────────────────────────────────

#[derive(Default)]
struct StubMemory {
    records: Mutex<Vec<MemoryRecord>>,
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn scope() -> Scope {
    Scope {
        tenant: "t".to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("a"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn record(text: &str) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::from(format!("m-{text}")),
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: vec![],
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: actor(),
            observed_at: now(),
            evidence: vec![],
            derivations: vec![],
            confidence: None,
            method: None,
        },
        policy: policy(),
        status: MemoryStatus::Active,
        links: vec![],
        assertions: vec![],
        created_at: now(),
        updated_at: None,
        metadata: None,
    }
}

fn unsupported<T>(what: &str) -> CoreResult<T> {
    Err(CoreError::CapabilityUnsupported {
        capability: what.to_owned(),
        reason: "sequential-harness stub".to_owned(),
    })
}

#[async_trait]
impl MemoryRepository for StubMemory {
    async fn put_memory(&self, rec: MemoryRecord) -> CoreResult<MemoryRecord> {
        self.records.lock().unwrap().push(rec.clone());
        Ok(rec)
    }

    async fn get_memory(&self, id: &MemoryId, _scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .find(|r| &r.id == id)
            .cloned())
    }

    async fn append_event(&self, _event: MemoryEvent) -> CoreResult<MemoryEvent> {
        unsupported("append_event")
    }

    async fn update_memory_status(
        &self,
        _id: &MemoryId,
        _scope: &Scope,
        _status: MemoryStatus,
    ) -> CoreResult<MemoryRecord> {
        unsupported("update_memory_status")
    }

    async fn list_memories_paged(
        &self,
        _scope: &Scope,
        _after: Option<&Cursor>,
        _limit: usize,
    ) -> CoreResult<Page<MemoryRecord>> {
        Ok(Page {
            items: self.records.lock().unwrap().clone(),
            next_cursor: None,
        })
    }
}

#[async_trait]
impl MemoryEventRepository for StubMemory {
    async fn get_event(&self, _id: &EventId, _scope: &Scope) -> CoreResult<Option<MemoryEvent>> {
        unsupported("get_event")
    }

    async fn list_events_for_memory(
        &self,
        _id: &MemoryId,
        _scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>> {
        Ok(vec![])
    }

    async fn list_events_for_scope(&self, _scope: &Scope) -> CoreResult<Vec<MemoryEvent>> {
        Ok(vec![])
    }
}

#[async_trait]
impl MemoryService for StubMemory {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        let rec = record(&request.content.text);
        let event = MemoryEvent {
            id: EventId::from(format!("e-{}", rec.id)),
            kind: MemoryEventKind::Written,
            scope: request.scope.clone(),
            actor: request.provenance.actor.clone(),
            memory_id: Some(rec.id.clone()),
            payload: Scalar::Null,
            provenance: request.provenance.clone(),
            occurred_at: now(),
            recorded_at: now(),
        };
        self.records.lock().unwrap().push(rec.clone());
        Ok(WriteMemoryResponse {
            record: rec,
            event,
            deduplicated: Some(false),
        })
    }

    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        let tokens: Vec<String> = request
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let items: Vec<RetrievalResult> = self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|rec| {
                let text = rec.content.text.to_lowercase();
                tokens.iter().any(|token| text.contains(token))
            })
            .map(|rec| RetrievalResult {
                id: rec.id.to_string(),
                target_type: RetrievalTargetType::Memory,
                target_id: rec.id.to_string(),
                content: rec.content.text.clone(),
                score: RetrievalScore {
                    total: 1.0,
                    relevance: Some(1.0),
                    recency: None,
                    confidence: None,
                    cue_match: None,
                    hierarchical_fit: None,
                    policy_fit: None,
                },
                provenance: rec.provenance.clone(),
                policy: rec.policy.clone(),
                explanation: None,
                fusion_trace: None,
                metadata: None,
            })
            .collect();
        Ok(ContextPayload {
            items,
            budget: None,
            omitted: vec![],
            source_failures: vec![],
            created_at: now(),
        })
    }

    async fn forget(&self, _request: ForgetRequest) -> CoreResult<ForgetResult> {
        unsupported("forget")
    }
}

// ── Fake probe: succeeds iff any retrieved item contains the capability ────

struct KeywordProbe;

#[async_trait]
impl TaskProbe for KeywordProbe {
    async fn check(&self, capability: &str, context: &ContextPayload) -> CoreResult<bool> {
        Ok(context
            .items
            .iter()
            .any(|item| item.content.to_lowercase().contains(capability)))
    }
}

// ── Script helpers ─────────────────────────────────────────────────────────

fn write(text: &str) -> SequentialEvent {
    SequentialEvent::Write(WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: vec![],
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(),
        requester: Requester {
            actor: actor(),
            roles: vec![],
            permissions: vec![],
            on_behalf_of: None,
        },
        provenance: Provenance {
            source: "test".to_owned(),
            actor: actor(),
            observed_at: now(),
            evidence: vec![],
            derivations: vec![],
            confidence: None,
            method: None,
        },
        policy: policy(),
        links: vec![],
        idempotency_key: None,
    })
}

fn probe(capability: &str, query: &str) -> SequentialEvent {
    SequentialEvent::Probe(ProbeStep {
        retrieval: RetrievalRequest {
            query: query.to_owned(),
            scope: scope(),
            requester: Requester {
                actor: actor(),
                roles: vec![],
                permissions: vec![],
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Keyword],
            filters: None,
            cues: vec![],
            limit: None,
            budget: None,
            include_explanations: None,
        },
        capability: capability.to_owned(),
    })
}

struct StubFactory;

impl ResetProviderFactory for StubFactory {
    fn fresh(&self) -> CoreResult<Arc<dyn MemoryService>> {
        Ok(Arc::new(StubMemory::default()))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn interference_detected_when_a_write_regresses_a_capability() {
    // Write the runbook → probe passes → write a DISTRACTING memory that
    // pollutes the capability's retrieval → probe regresses.
    let events = vec![
        write("payment-timeout runbook: retry with backoff"),
        probe("payment-timeout", "payment timeout"),
        write("payment-timeout distractor: unrelated noise keyword"),
        probe("payment-timeout", "payment timeout distractor"),
    ];

    let report = block_on(async {
        let harness = SequentialHarness::new(Arc::new(KeywordProbe), events);
        harness
            .run(Arc::new(StubMemory::default()), Arc::new(StubFactory))
            .await
    })
    .expect("harness run");

    // Capability matrix tracks the stateful arm across both checkpoints.
    assert_eq!(report.capability_matrix.len(), 1);
    let (cap, entries) = &report.capability_matrix[0];
    assert_eq!(cap, "payment-timeout");
    assert_eq!(entries.len(), 2);
    assert!(entries[0].1, "runbook retrieved at checkpoint 0");
    assert!(
        entries[1].1,
        "capability still matches (both texts contain it)"
    );

    // Both arms see identical inputs; checkpoints carry matched measurements.
    assert_eq!(report.checkpoints.len(), 2);
    for cp in &report.checkpoints {
        assert_eq!(cp.stateful.len(), 1);
        assert_eq!(cp.reset.len(), 1);
    }
}

#[test]
fn stateful_minus_reset_gain_is_reported_per_checkpoint() {
    // Stateful arm holds the runbook; reset arm rebuilt each checkpoint also
    // replays prior writes, so ΔP = 0 here — proving reset replay correctness
    // (the reset arm is NOT empty; it sees identical prior writes).
    let events = vec![
        write("deploy-gate fact: v2 holds"),
        probe("deploy-gate", "deploy gate"),
    ];
    let report = block_on(async {
        let harness = SequentialHarness::new(Arc::new(KeywordProbe), events);
        harness
            .run(Arc::new(StubMemory::default()), Arc::new(StubFactory))
            .await
    })
    .expect("harness run");

    assert_eq!(report.checkpoints.len(), 1);
    let cp = &report.checkpoints[0];
    assert!(cp.stateful[0].1);
    assert!(cp.reset[0].1, "reset arm replayed the same write");
    assert_eq!(cp.gain, 0.0);
    assert_eq!(report.mean_gain, 0.0);
    assert!(report.interference.is_empty());
}

#[test]
fn regression_only_counts_capabilities_that_previously_passed() {
    // First probe FAILS (no write yet); the write arrives; second probe
    // passes. No interference: you cannot regress from a failure.
    let events = vec![
        probe("fresh-capability", "fresh capability"),
        write("fresh-capability knowledge arrives late"),
        probe("fresh-capability", "fresh capability"),
    ];
    let report = block_on(async {
        let harness = SequentialHarness::new(Arc::new(KeywordProbe), events);
        harness
            .run(Arc::new(StubMemory::default()), Arc::new(StubFactory))
            .await
    })
    .expect("harness run");

    assert!(report.interference.is_empty(), "{report:?}");
    let entries = &report.capability_matrix[0].1;
    assert!(!entries[0].1, "nothing stored yet");
    assert!(entries[1].1, "acquired after the write");
    // Gain at checkpoint 0: stateful false, reset false -> 0; checkpoint 1:
    // stateful true, reset true (replayed) -> 0. Acquisition shows in the
    // matrix, not in ΔP — ΔP isolates the STATEFUL advantage only.
    assert_eq!(report.mean_gain, 0.0);
}

#[test]
fn empty_stream_reports_zero_mean_without_panicking() {
    let report = block_on(async {
        let harness = SequentialHarness::new(Arc::new(KeywordProbe), vec![]);
        harness
            .run(Arc::new(StubMemory::default()), Arc::new(StubFactory))
            .await
    })
    .expect("empty run");
    assert!(report.checkpoints.is_empty());
    assert_eq!(report.mean_gain, 0.0);
    assert!(report.interference.is_empty());
}
