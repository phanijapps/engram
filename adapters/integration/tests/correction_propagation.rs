//! Correction-propagation regression (MCCL-audit F2, slice 2C).
//!
//! End-to-end on real stores: write memories → synthesize derived beliefs
//! that CITE them (lineage from slice 2B) → redact one memory through
//! `MemoryService::forget` → `DerivedBeliefLifecycle::stale_for_memory`
//! marks the citing belief stale; beliefs citing OTHER memories stay live.

use std::sync::Arc;

use engram_belief::BeliefRepository;
use engram_belief::BeliefSynthesizer;
use engram_domain::*;
use engram_integration::DerivedBeliefLifecycle;
use engram_memory::MemoryService;
use engram_reflection::ActiveMemoryEvidence;
use engram_reflection::{ActiveMemorySource, PatternSynthesizer};
use engram_store_sqlite::{SqlBeliefStore, SqlMemoryService};
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-propagation".to_owned(),
        subject: None,
        workspace: Some("w".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn requester() -> Requester {
    Requester {
        actor: Actor {
            id: Id::from("test-agent"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        roles: vec![],
        permissions: vec![],
        on_behalf_of: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Consolidation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn write_request(text: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
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
        requester: requester(),
        provenance: Provenance {
            source: "test".to_owned(),
            actor: Actor {
                id: Id::from("test-agent"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: vec![],
            derivations: vec![],
            confidence: None,
            method: None,
        },
        policy: policy(),
        links: vec![],
        verdict: None,
        idempotency_key: None,
    }
}

/// Fixed evidence source (stores are exercised separately around it).
struct FixedSource(Vec<ActiveMemoryEvidence>);

#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
impl ActiveMemorySource for FixedSource {
    async fn active_memories(
        &self,
        _scope: &Scope,
    ) -> engram_runtime::CoreResult<Vec<ActiveMemoryEvidence>> {
        Ok(self.0.clone())
    }
}

#[test]
fn redaction_stales_only_beliefs_citing_the_corrected_memory() {
    let memory = SqlMemoryService::open_in_memory().expect("memory service");
    let beliefs = Arc::new(SqlBeliefStore::open_in_memory().expect("belief store"));

    // Two memories on one topic; a third on another → two derived beliefs.
    let first =
        block_on(memory.write_memory(write_request("GOTCHAS: budget limits bite at the composer")))
            .expect("write first");
    let second = block_on(memory.write_memory(write_request("GOTCHAS: second memory same topic")))
        .expect("write second");
    let other =
        block_on(memory.write_memory(write_request("ARCHITECTURE: unrelated topic entirely")))
            .expect("write other");

    let evidence_of = |record: &MemoryRecord| ActiveMemoryEvidence {
        memory_id: record.id.clone(),
        text: record.content.text.clone(),
        policy: record.policy.clone(),
        observed_at: record.provenance.observed_at,
    };
    let evidence = vec![
        evidence_of(&first.record),
        evidence_of(&second.record),
        evidence_of(&other.record),
    ];

    let request = ConsolidationRequest {
        scope: scope(),
        requester: requester(),
        since: None,
        until: None,
        strategy: Some(ConsolidationStrategy::Hybrid),
        dry_run: Some(false),
    };
    let synthesizer = PatternSynthesizer::new(Arc::new(FixedSource(evidence)), chrono::Utc::now());
    let derived = block_on(synthesizer.synthesize_beliefs(&request)).expect("synthesize");
    assert_eq!(derived.len(), 2, "two topics -> two beliefs");
    for belief in &derived {
        block_on(beliefs.put_belief(belief.clone())).expect("persist belief");
    }

    // Lineage precondition: the GOTCHAS belief cites BOTH gotchas memories.
    let all = block_on(beliefs.list_beliefs(&scope())).expect("list beliefs");
    let gotchas = all
        .iter()
        .find(|b| b.subject.key.starts_with("reflection:gotchas"))
        .expect("gotchas belief");
    assert_eq!(gotchas.sources.len(), 2, "lineage cites both inputs");
    assert!(
        gotchas
            .sources
            .iter()
            .any(|s| s.target_id == first.record.id.to_string())
    );

    // Redact the FIRST memory through the normal forget path.
    let forget = ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: first.record.id.to_string(),
        scope: scope(),
        requester: requester(),
        mode: DeleteMode::Redact,
        reason: Some("correction".to_owned()),
    };
    block_on(memory.forget(forget)).expect("redact");

    // Propagation: only the GOTCHAS belief (citing the redacted memory) stales.
    let lifecycle = DerivedBeliefLifecycle::new(beliefs.clone());
    let staled =
        block_on(lifecycle.stale_for_memory(&first.record.id, &scope())).expect("propagate");
    assert_eq!(staled.len(), 1, "exactly the citing belief stales");

    let all = block_on(beliefs.list_beliefs(&scope())).expect("list beliefs");
    let gotchas = all
        .iter()
        .find(|b| b.subject.key.starts_with("reflection:gotchas"))
        .expect("gotchas belief");
    let architecture = all
        .iter()
        .find(|b| b.subject.key.starts_with("reflection:architecture"))
        .expect("architecture belief");
    assert_eq!(
        gotchas.status,
        BeliefStatus::Stale,
        "citing belief is stale"
    );
    assert_eq!(
        architecture.status,
        BeliefStatus::Active,
        "unrelated belief stays live"
    );

    // Idempotent: re-running propagation stales nothing new.
    let again =
        block_on(lifecycle.stale_for_memory(&first.record.id, &scope())).expect("re-propagate");
    assert!(again.is_empty(), "already-stale beliefs are not re-touched");
}
