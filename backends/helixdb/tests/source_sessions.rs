#![cfg(any(feature = "embedded", feature = "service"))]
#[path = "../../../adapters/helixdb/tests/support/data_fixture.rs"]
mod fixture;
use engram_backend_helixdb::composition::HelixComposition;
use engram_domain::*;
use engram_integration::storage_composition::{ObservedPorts, StorageComposition};
use engram_memory::{MemoryRepository, MemoryService};
use engram_runtime::{CoreResult, PolicyAuthorizer};
use fixture::*;
use futures::executor::block_on;
use std::sync::Arc;
struct Allow;
impl PolicyAuthorizer for Allow {
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
fn write(key: &str, text: &str) -> WriteMemoryRequest {
    let record = memory("request-template");
    WriteMemoryRequest {
        kind: record.kind,
        content: MemoryContent {
            text: text.into(),
            ..record.content
        },
        scope: scope(),
        requester: Requester {
            actor: record.provenance.actor.clone(),
            roles: vec![],
            permissions: vec!["memory.write".into()],
            on_behalf_of: None,
        },
        provenance: record.provenance,
        policy: record.policy,
        links: vec![],
        verdict: None,
        idempotency_key: Some(key.into()),
    }
}
fn nested_memory(mode: Mode) {
    let mut fixture = Fixture::new(mode);
    #[cfg(feature = "test-controls")]
    let mut control = fixture.store.instrument_native_test_control().unwrap();
    let composition = HelixComposition::new(fixture.store.clone(), Arc::new(Allow));
    let observed = ObservedPorts::new(composition.session(&scope()).unwrap());
    let result = block_on(observed.write_memory(write("bound-user-key", "nested canonical write")));
    let stored = result
        .as_ref()
        .ok()
        .map(|r| block_on(fixture.store.get_memory(&r.record.id, &scope())).unwrap());
    drop(observed);
    drop(composition);
    if result.is_err() {
        fixture.store.close().unwrap();
    }
    assert!(
        result.is_ok(),
        "canonical nested memory call must borrow its validated outer item frame: {result:?}"
    );
    assert!(
        stored.flatten().is_some(),
        "acknowledged write must be durable"
    );
    #[cfg(feature = "test-controls")]
    full_session_contracts(&mut fixture, &mut control);
    fixture.store.close().unwrap();
}
#[cfg(feature = "embedded")]
#[test]
fn embedded_canonical_session_preserves_nested_write_identity() {
    nested_memory(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_canonical_session_preserves_nested_write_identity() {
    nested_memory(Mode::Service)
}

#[cfg(feature = "test-controls")]
fn full_session_contracts(
    f: &mut Fixture,
    control: &mut Arc<engram_store_helixdb::composition_storage::test_controls::TestControl>,
) {
    use engram_integration::{
        BatchIngestRequest, BatchStep, CapabilityPolicy, EmbeddingProviderConfig, EngramConfig,
        MigrationMode, StepStatus,
    };
    use engram_knowledge::{KnowledgeRepository, SourceBatchLedger};
    use engram_memory::{MemoryEventRepository, MemoryStorage};
    use engram_runtime::effect::EffectOutcome;
    let cache = tempfile::tempdir().unwrap();
    let config = EngramConfig::new(
        cache.path().join("recipe"),
        cache.path(),
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".into(),
            dimensions: 4,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_enable_vector(true);
    let mut provider =
        Some(engram_backend_helixdb::compose(f.store.clone(), &config, Arc::new(Allow)).unwrap());
    let composition = HelixComposition::new(f.store.clone(), Arc::new(Allow));
    // ID-free reads, stale handles, explicit actor/item mismatch and abandonment.
    {
        let session = f.store.native_session(&scope()).unwrap();
        let call = session.begin_call("actor", "read").result.unwrap();
        let before = control.snapshot();
        call.complete(Ok(())).unwrap();
        let observed = call.observation(false).unwrap();
        assert_eq!(observed.outcome, EffectOutcome::Acknowledged);
        assert_eq!(observed.commit_id, None);
        assert_eq!(observed.step, None);
        assert!(call.observation(true).is_err());
        let next = session.begin_call("actor", "next").result.unwrap();
        assert!(call.validate_live().is_err());
        next.complete(Ok(())).unwrap();
        assert_eq!(
            control.snapshot().mutation_attempts,
            before.mutation_attempts
        );
    }
    {
        let session = f.store.native_session(&scope()).unwrap();
        let call = session.begin_call("actor", "abandoned").result.unwrap();
        let before = control.snapshot();
        block_on(call.store().put_entity(entity("abandoned", &[]))).unwrap();
        assert!(call.observation(false).is_err());
        drop(call);
        drop(session);
        assert!(
            block_on(f.store.get_entity(&"abandoned".into(), &scope()))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            control.snapshot().mutation_attempts,
            before.mutation_attempts
        );
    }
    for (actor, item) in [("foreign-actor", "semantic-key"), ("actor", "foreign-item")] {
        let session = f.store.native_session(&scope()).unwrap();
        let call = session.begin_call(actor, item).result.unwrap();
        let before = control.snapshot();
        let result = block_on(
            call.store()
                .memory_service(Arc::new(Allow))
                .write_memory(write("semantic-key", "must reject")),
        );
        assert!(call.complete(result).is_err());
        let observation = call.observation(true).unwrap();
        assert_eq!(observation.outcome, EffectOutcome::DefiniteFailure);
        assert_eq!(observation.commit_id, None);
        assert_eq!(
            control.snapshot().mutation_attempts,
            before.mutation_attempts
        );
    }
    let mut foreign = scope();
    foreign.session = Some(String::new());
    let before = control.snapshot();
    assert!(composition.session(&foreign).is_err());
    assert_eq!(control.snapshot().all_sends, before.all_sends);
    drop(composition);
    // Public direct memory and knowledge use the real retained transport.
    control.lose_next_mutation_ack();
    let error = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_memory()
            .unwrap()
            .write_memory(write("direct-lost", "private-record-marker")),
    )
    .unwrap_err();
    let id = control.snapshot().landed_commits.last().unwrap().clone();
    let before = control.snapshot();
    let blocked = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_memory()
            .unwrap()
            .write_memory(write("direct-blocked", "must not send")),
    )
    .unwrap_err();
    assert_eq!(control.snapshot().all_sends, before.all_sends);
    assert_eq!(
        control.snapshot().mutation_attempts,
        before.mutation_attempts
    );
    assert!(blocked.to_redacted().to_string().contains(&id));
    let receipt = resolve_and_reopen(f, control, &mut provider, &config, &id);
    assert_eq!(receipt["id"], id);
    assert!(error.to_redacted().to_string().contains(&id));
    assert!(
        !error
            .to_redacted()
            .to_string()
            .contains("private-record-marker")
    );
    assert!(
        block_on(f.store.idempotent_response(&scope(), "direct-lost"))
            .unwrap()
            .is_some()
    );
    control.lose_next_mutation_ack();
    let error = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_knowledge()
            .unwrap()
            .put_entity(entity("direct-knowledge", &[])),
    )
    .unwrap_err();
    let id = control.snapshot().landed_commits.last().unwrap().clone();
    let before = control.snapshot();
    assert!(
        block_on(
            provider
                .as_ref()
                .unwrap()
                .require_knowledge()
                .unwrap()
                .put_entity(entity("direct-knowledge-blocked", &[]))
        )
        .is_err()
    );
    assert_eq!(control.snapshot().all_sends, before.all_sends);
    let receipt = resolve_and_reopen(f, control, &mut provider, &config, &id);
    assert_eq!(receipt["id"], id);
    assert!(error.to_redacted().to_string().contains(&id));
    assert!(
        block_on(f.store.get_entity(&"direct-knowledge".into(), &scope()))
            .unwrap()
            .is_some()
    );
    assert!(
        block_on(
            f.store
                .get_entity(&"direct-knowledge-blocked".into(), &scope())
        )
        .unwrap()
        .is_none()
    );
    // Exact operation/session-owned frames match the actual commit, not arbitrary receipts.
    for knowledge_call in [false, true] {
        let composition = HelixComposition::new(f.store.clone(), Arc::new(Allow));
        let observed = ObservedPorts::new(composition.session(&scope()).unwrap());
        control.lose_next_mutation_ack();
        let failed = if knowledge_call {
            block_on(observed.put_entity(entity("observed-knowledge", &[]))).map(|_| ())
        } else {
            block_on(observed.write_memory(write("observed-memory", "observed memory"))).map(|_| ())
        };
        assert!(failed.is_err());
        let observation = observed.state.lock().unwrap().stopped.clone().unwrap();
        let id = control.snapshot().landed_commits.last().unwrap().clone();
        assert_eq!(observation.commit_id.as_deref(), Some(id.as_str()));
        assert!(observation.step.is_some());
        assert_eq!(
            observation.outcome,
            EffectOutcome::Unknown {
                operation_id: id.clone()
            }
        );
        assert_eq!(observation.identity.scope, scope());
        assert_eq!(observation.identity.backend, "helixdb");
        assert_eq!(observation.identity.store, f.partition.logical_store);
        assert_eq!(
            observation.identity.mode,
            match f.mode {
                Mode::Embedded => "embedded",
                Mode::Service => "service",
            }
        );
        assert!(!observation.identity.session.is_empty() && !observation.identity.call.is_empty());
        let before = control.snapshot();
        assert!(block_on(observed.put_entity(entity("observed-blocked", &[]))).is_err());
        assert_eq!(control.snapshot().all_sends, before.all_sends);
        drop(observed);
        drop(composition);
        let session = f.store.native_session(&scope());
        assert!(session.is_err());
        assert_eq!(
            control.snapshot().mutation_attempts,
            before.mutation_attempts
        );
        let receipt = resolve_and_reopen(f, control, &mut provider, &config, &id);
        assert_eq!(receipt["id"], id);
    }
    let request = |key: &str,
                   facts: Vec<MemoryRecord>,
                   entities: Vec<KnowledgeEntity>,
                   source: Option<&str>| BatchIngestRequest {
        idempotency_key: key.into(),
        scope: scope(),
        source: None,
        documents: vec![],
        chunks: vec![],
        facts,
        entities,
        relationships: vec![],
        evidence: vec![],
        embeddings: vec![],
        source_id: source.map(str::to_owned),
    };
    // Earlier definite failures cannot mask later uncertainty; both native families stop.
    for knowledge_call in [false, true] {
        let key = if knowledge_call {
            "knowledge-batch"
        } else {
            "memory-batch"
        };
        let mut invalid_entity = entity("", &[]);
        invalid_entity.name = "invalid identity".into();
        let mut bad = memory("bad");
        bad.content.text.clear();
        let facts = if knowledge_call {
            vec![]
        } else {
            vec![memory("first"), bad, memory("landed"), memory("later")]
        };
        let entities = if knowledge_call {
            vec![
                entity("knowledge-first", &[]),
                invalid_entity,
                entity("knowledge-landed", &[]),
                entity("knowledge-later", &[]),
            ]
        } else {
            vec![entity("later-step", &[])]
        };
        control.lose_mutation_ack_after(1);
        let outcome = block_on(
            provider
                .as_ref()
                .unwrap()
                .require_batch()
                .unwrap()
                .ingest(request(key, facts, entities, None)),
        )
        .unwrap();
        let id = control.snapshot().landed_commits.last().unwrap().clone();
        let failed_step = if knowledge_call {
            BatchStep::Entities
        } else {
            BatchStep::Facts
        };
        let step = outcome
            .steps
            .iter()
            .find(|s| s.step == failed_step)
            .unwrap();
        assert_eq!(step.status, StepStatus::Failed);
        assert!(
            step.error
                .as_ref()
                .unwrap()
                .to_redacted()
                .to_string()
                .contains(&id)
        );
        let before = control.snapshot();
        assert!(
            block_on(
                provider
                    .as_ref()
                    .unwrap()
                    .require_batch()
                    .unwrap()
                    .ingest(request(
                        "blocked-batch",
                        vec![memory("blocked")],
                        vec![],
                        None
                    ))
            )
            .is_err()
        );
        assert_eq!(control.snapshot().all_sends, before.all_sends);
        assert_eq!(
            control.snapshot().mutation_attempts,
            before.mutation_attempts
        );
        resolve_and_reopen(f, control, &mut provider, &config, &id);
        if knowledge_call {
            assert!(
                block_on(f.store.get_entity(&"knowledge-first".into(), &scope()))
                    .unwrap()
                    .is_some()
            );
            assert!(
                block_on(f.store.get_entity(&"knowledge-landed".into(), &scope()))
                    .unwrap()
                    .is_some()
            );
            assert!(
                block_on(f.store.get_entity(&"knowledge-later".into(), &scope()))
                    .unwrap()
                    .is_none()
            );
        } else {
            assert!(
                block_on(f.store.idempotent_response(&scope(), "memory-batch#0"))
                    .unwrap()
                    .is_some()
            );
            assert!(
                block_on(f.store.idempotent_response(&scope(), "memory-batch#2"))
                    .unwrap()
                    .is_some()
            );
            assert!(
                block_on(f.store.idempotent_response(&scope(), "memory-batch#3"))
                    .unwrap()
                    .is_none()
            );
            assert!(
                block_on(f.store.get_entity(&"later-step".into(), &scope()))
                    .unwrap()
                    .is_none()
            );
        }
    }
    // A known definite transport rejection continues ordinary BestEffort.
    control.reject_next_mutation_definitely();
    let outcome = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_batch()
            .unwrap()
            .ingest(request(
                "definite",
                vec![memory("reject"), memory("continue")],
                vec![entity("definite-next-step", &[])],
                None,
            )),
    )
    .unwrap();
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Facts)
            .unwrap()
            .status,
        StepStatus::Failed
    );
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Entities)
            .unwrap()
            .status,
        StepStatus::Succeeded
    );
    assert!(
        block_on(f.store.idempotent_response(&scope(), "definite#0"))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(f.store.idempotent_response(&scope(), "definite#1"))
            .unwrap()
            .is_some()
    );
    // A sourced landed write stops before its ledger, preserving the succeeded episode.
    control.lose_mutation_ack_after(1);
    let outcome = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_batch()
            .unwrap()
            .ingest(request(
                "sourced-loss",
                vec![memory("sourced")],
                vec![entity("sourced-later", &[])],
                Some("ledger-owned"),
            )),
    )
    .unwrap();
    let id = control.snapshot().landed_commits.last().unwrap().clone();
    assert_eq!(outcome.steps[0].status, StepStatus::Succeeded);
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Entities)
            .unwrap()
            .status,
        StepStatus::Skipped
    );
    resolve_and_reopen(f, control, &mut provider, &config, &id);
    assert!(
        block_on(f.store.list_source_batch_records(&scope(), "ledger-owned"))
            .unwrap()
            .is_empty()
    );
    assert!(
        block_on(f.store.get_entity(&"sourced-later".into(), &scope()))
            .unwrap()
            .is_none()
    );
    // Uncertainty while retiring a prior fact cannot mark its ledger or send replacement work.
    block_on(
        provider
            .as_ref()
            .unwrap()
            .require_batch()
            .unwrap()
            .ingest(request(
                "seed-revision",
                vec![memory("old")],
                vec![],
                Some("supersede-owned"),
            )),
    )
    .unwrap();
    let prior = block_on(
        f.store
            .list_source_batch_records(&scope(), "supersede-owned"),
    )
    .unwrap();
    assert_eq!(prior.len(), 1);
    let old_id = MemoryId::from(prior[0].record_id.clone());
    assert_eq!(
        block_on(f.store.get_memory(&old_id, &scope()))
            .unwrap()
            .unwrap()
            .status,
        MemoryStatus::Active
    );
    let revision = request(
        "next-revision",
        vec![memory("replacement")],
        vec![entity("revision-later", &[])],
        Some("supersede-owned"),
    );
    // Episode has exactly one registry upsert; the next mutation is the
    // prior fact's tombstone, before ledger retirement or replacement writes.
    assert!(
        revision.source.is_none() && revision.documents.is_empty() && revision.chunks.is_empty()
    );
    assert_eq!(revision.source_id.as_deref(), Some("supersede-owned"));
    let before_revision = control.snapshot();
    control.lose_mutation_ack_after(1);
    let outcome = block_on(
        provider
            .as_ref()
            .unwrap()
            .require_batch()
            .unwrap()
            .ingest(revision),
    )
    .unwrap();
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Episode)
            .unwrap()
            .status,
        StepStatus::Succeeded
    );
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Facts)
            .unwrap()
            .status,
        StepStatus::Failed
    );
    assert_eq!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Entities)
            .unwrap()
            .status,
        StepStatus::Skipped
    );
    let after_revision = control.snapshot();
    assert_eq!(
        after_revision.mutation_attempts.len(),
        before_revision.mutation_attempts.len() + 2
    );
    assert_eq!(
        after_revision.landed_commits.len(),
        before_revision.landed_commits.len() + 2
    );
    let id = control.snapshot().landed_commits.last().unwrap().clone();
    assert!(
        outcome
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Facts)
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .to_redacted()
            .to_string()
            .contains(&id)
    );
    resolve_and_reopen(f, control, &mut provider, &config, &id);
    assert_eq!(
        block_on(f.store.get_memory(&old_id, &scope()))
            .unwrap()
            .unwrap()
            .status,
        MemoryStatus::Forgotten
    );
    assert_eq!(
        block_on(
            f.store
                .list_source_batch_records(&scope(), "supersede-owned")
        )
        .unwrap(),
        prior
    );
    assert!(
        block_on(f.store.idempotent_response(&scope(), "next-revision#0"))
            .unwrap()
            .is_none()
    );
    assert!(
        block_on(f.store.get_entity(&"revision-later".into(), &scope()))
            .unwrap()
            .is_none()
    );
    source_barriers(f, control, provider.as_ref().unwrap());
    let events = block_on(f.store.list_events_for_scope(&scope())).unwrap();
    assert!(!events.is_empty());
    eprintln!(
        "HELIX_RECIPE_SESSION_PROOF mode={:?} actual_mutations={} landed_receipts={} full_scope={:?}",
        f.mode,
        control.snapshot().mutation_attempts.len(),
        control.snapshot().landed_commits.len(),
        scope()
    );
    drop(provider);
}

#[cfg(feature = "test-controls")]
fn resolve_and_reopen(
    f: &mut Fixture,
    control: &mut Arc<engram_store_helixdb::composition_storage::test_controls::TestControl>,
    provider: &mut Option<engram_integration::EngramProvider>,
    config: &engram_integration::EngramConfig,
    id: &str,
) -> serde_json::Value {
    assert!(
        f.store.close().is_err(),
        "unresolved injected acknowledgement must retain its owner"
    );
    let before = control.snapshot();
    let receipt = f.store.reconcile_operation(id).unwrap();
    assert_eq!(receipt["id"], id);
    let after = control.snapshot();
    assert_eq!(after.reconciliation_reads, before.reconciliation_reads + 1);
    assert_eq!(after.all_sends, before.all_sends + 1);
    assert_eq!(after.mutation_attempts, before.mutation_attempts);
    assert_eq!(after.landed_commits, before.landed_commits);
    eprintln!(
        "HELIX_RECIPE_RECEIPT_VERIFIED mode={:?} operation={} receipt_read_dispatches=1 new_mutations=0",
        f.mode, id
    );
    drop(provider.take());
    // Reconciliation authorizes cleanup only; the retained owner never resumes logical work.
    f.reopen();
    *control = f.store.instrument_native_test_control().unwrap();
    *provider =
        Some(engram_backend_helixdb::compose(f.store.clone(), config, Arc::new(Allow)).unwrap());
    receipt
}

#[cfg(feature = "test-controls")]
fn source_barriers(
    f: &Fixture,
    control: &Arc<engram_store_helixdb::composition_storage::test_controls::TestControl>,
    provider: &engram_integration::EngramProvider,
) {
    use engram_integration::{BatchIngestRequest, StepStatus};
    use engram_knowledge::KnowledgeRepository;
    block_on(f.store.put_entity(entity("left", &[]))).unwrap();
    block_on(f.store.put_entity(entity("right", &[]))).unwrap();
    for (index, source_b) in ["source-a", "source-b"].into_iter().enumerate() {
        let request = |key: &str, source: &str| BatchIngestRequest {
            idempotency_key: key.into(),
            scope: scope(),
            source: None,
            documents: vec![],
            chunks: vec![],
            facts: vec![],
            entities: vec![entity("shared", &[])],
            relationships: vec![relation("shared-edge", &[])],
            evidence: vec![],
            embeddings: vec![],
            source_id: Some(source.into()),
        };
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        // First round includes source registration; the second source-a already exists.
        control.pause_mutation_after(if index == 0 { 1 } else { 0 }, entered_tx, release_rx);
        let batch = provider.require_batch().unwrap().clone();
        let first = request(&format!("barrier-first-{index}"), "source-a");
        let worker = std::thread::spawn(move || block_on(batch.ingest(first)));
        let entered = entered_rx.recv_timeout(std::time::Duration::from_secs(120));
        let before = control.snapshot();
        let blocked = block_on(
            provider
                .require_batch()
                .unwrap()
                .ingest(request(&format!("barrier-second-{index}"), source_b)),
        );
        let after = control.snapshot();
        let _ = release_tx.send(());
        let first = worker.join().unwrap();
        assert!(entered.is_ok());
        assert!(blocked.is_err());
        assert_eq!(before.all_sends, after.all_sends);
        assert!(
            first
                .unwrap()
                .steps
                .iter()
                .all(|s| s.status != StepStatus::Failed)
        );
        let second = block_on(
            provider
                .require_batch()
                .unwrap()
                .ingest(request(&format!("barrier-second-{index}"), source_b)),
        )
        .unwrap();
        assert!(second.steps.iter().all(|s| s.status != StepStatus::Failed));
    }
    let shared = block_on(f.store.get_entity(&"shared".into(), &scope()))
        .unwrap()
        .unwrap();
    for source in ["source-a", "source-b"] {
        assert!(
            shared
                .source_refs
                .iter()
                .any(|r| r.target_id.as_deref() == Some(source))
        );
    }
    let edge = block_on(f.store.get_relationship(&"shared-edge".into(), &scope()))
        .unwrap()
        .unwrap();
    for source in ["source-a", "source-b"] {
        assert!(
            edge.evidence
                .iter()
                .any(|r| r.target_id.as_deref() == Some(source))
        );
    }
}
