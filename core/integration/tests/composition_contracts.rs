#![cfg(feature = "sqlite")]
use engram_domain::*;
use engram_integration::batch_runtime::SharedBatchIngest;
use engram_integration::storage_composition::*;
use engram_integration::{BatchIngest, BatchIngestRequest, BatchStep, StepStatus};
use engram_runtime::{CoreResult, effect::*};
use engram_store_sqlite::{SqlKnowledgeStore, SqlMemoryService};
use futures::executor::block_on;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
const ORIGINAL: &str = "b5e12447-4377-4c90-a40f-84cc19436748";
struct Factory {
    ports: CallPorts,
    scope: Scope,
    calls: Arc<AtomicUsize>,
    unknown_at: usize,
    blocked_at: usize,
    mismatch: Option<&'static str>,
}
struct Session {
    ports: CallPorts,
    scope: Scope,
    calls: Arc<AtomicUsize>,
    unknown_at: usize,
    blocked_at: usize,
    mismatch: Option<&'static str>,
}
struct Observer {
    identity: EffectIdentity,
    index: usize,
    unknown_at: usize,
    mismatch: Option<&'static str>,
}
impl CallObservation for Observer {
    fn finish(&self, failed: bool) -> CoreResult<EffectObservation> {
        let mut identity = self.identity.clone();
        if self.mismatch == Some("observation-call") {
            identity.call.push_str("-foreign");
        }
        let mut observation = EffectObservation {
            identity,
            step: Some(format!("step-{}", self.index)),
            commit_id: (self.index == self.unknown_at).then(|| ORIGINAL.into()),
            outcome: if self.index == self.unknown_at {
                EffectOutcome::Unknown {
                    operation_id: ORIGINAL.into(),
                }
            } else if failed {
                EffectOutcome::DefiniteFailure
            } else {
                EffectOutcome::Acknowledged
            },
            cleanup: CleanupCertainty::Confirmed,
        };
        match self.mismatch {
            Some("unknown-malformed") => {
                observation.outcome = EffectOutcome::Unknown {
                    operation_id: "credential-secret-private-record-/owned/path".into(),
                }
            }
            Some("unknown-foreign-commit") => {
                observation.commit_id = Some("0152c084-4ca6-431c-84f2-227a03abf5a5".into())
            }
            Some("unknown-missing-step") => observation.step = None,
            Some("unknown-empty-step") => observation.step = Some(String::new()),
            Some("unknown-missing-commit") => observation.commit_id = None,
            Some("unknown-uppercase") => {
                let id = ORIGINAL.to_uppercase();
                observation.commit_id = Some(id.clone());
                observation.outcome = EffectOutcome::Unknown { operation_id: id };
            }
            Some("unknown-wrong-version") => {
                let id = ORIGINAL.replacen("-4c90-", "-5c90-", 1);
                observation.commit_id = Some(id.clone());
                observation.outcome = EffectOutcome::Unknown { operation_id: id };
            }
            Some("cancelled-with-commit") => observation.outcome = EffectOutcome::Cancelled,
            Some("blocked-with-commit") => {
                observation.outcome = EffectOutcome::BlockedByPriorUnknown {
                    operation_id: ORIGINAL.into(),
                }
            }
            Some("blocked-with-step") => {
                observation.commit_id = None;
                observation.outcome = EffectOutcome::BlockedByPriorUnknown {
                    operation_id: ORIGINAL.into(),
                };
            }
            Some("definite-malformed-commit") => {
                observation.commit_id = Some("credential-secret-private-record".into());
                observation.outcome = EffectOutcome::DefiniteFailure;
            }
            Some("ack-without-step") => {
                observation.step = None;
                observation.outcome = EffectOutcome::Acknowledged;
            }
            Some("staged-malformed-commit") => {
                observation.commit_id = Some("credential-secret-private-record".into());
                observation.outcome = EffectOutcome::Staged;
            }
            _ => {}
        }
        Ok(observation)
    }
}
impl StorageComposition for Factory {
    fn session(&self, scope: &Scope) -> CoreResult<Arc<dyn CompositionSession>> {
        assert_eq!(scope, &self.scope);
        Ok(Arc::new(Session {
            ports: self.ports.clone(),
            scope: scope.clone(),
            calls: self.calls.clone(),
            unknown_at: self.unknown_at,
            blocked_at: self.blocked_at,
            mismatch: self.mismatch,
        }))
    }
}
impl CompositionSession for Session {
    fn binding(&self) -> SessionBinding {
        SessionBinding {
            backend: "typed-contract-sql".into(),
            mode: "local".into(),
            store: "owned-memory-knowledge".into(),
            session: "owned-contract-session".into(),
            scope: self.scope.clone(),
        }
    }

    fn scope(&self) -> &Scope {
        &self.scope
    }
    fn begin_call(&self, actor: &str, item: &str) -> ObservedResult<CompositionCall> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut identity = EffectIdentity {
            actor: actor.into(),
            item: item.into(),
            call: format!("call-{index}"),
            session: "owned-contract-session".into(),
            backend: "typed-contract-sql".into(),
            mode: "local".into(),
            store: "owned-memory-knowledge".into(),
            scope: self.scope.clone(),
        };
        match self.mismatch {
            Some("backend") => identity.backend.push('x'),
            Some("mode") => identity.mode.push('x'),
            Some("store") => identity.store.push('x'),
            Some("session") => identity.session.push('x'),
            Some("actor") => identity.actor.push('x'),
            Some("item") => identity.item.push('x'),
            Some("scope") => identity.scope.environment = Some("foreign".into()),
            _ => {}
        }
        if index == self.blocked_at {
            return ObservedResult {
                result: Err(engram_runtime::CoreError::Adapter {
                    adapter: "typed-contract-sql".into(),
                    message: format!("blocked by prior uncertain operation: {ORIGINAL}"),
                }),
                observation: EffectObservation {
                    identity,
                    step: None,
                    commit_id: None,
                    outcome: EffectOutcome::BlockedByPriorUnknown {
                        operation_id: ORIGINAL.into(),
                    },
                    cleanup: CleanupCertainty::Unconfirmed,
                },
            };
        }
        ObservedResult {
            result: Ok(CompositionCall {
                ports: self.ports.clone(),
                identity: identity.clone(),
                observation: Arc::new(Observer {
                    identity: identity.clone(),
                    index,
                    unknown_at: self.unknown_at,
                    mismatch: self.mismatch,
                }),
            }),
            observation: EffectObservation {
                identity,
                step: None,
                commit_id: None,
                outcome: EffectOutcome::Staged,
                cleanup: CleanupCertainty::Confirmed,
            },
        }
    }
}
fn fact(id: &str, text: &str, scope: &Scope) -> MemoryRecord {
    serde_json::from_value(json!({"id":id,"kind":"fact","content":{"text":text},"scope":scope,"status":"active","createdAt":"2026-11-17T00:00:00Z","provenance":{"source":"contract","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}})).unwrap()
}
fn setup_mode(
    unknown_at: usize,
    blocked_at: usize,
    mismatch: Option<&'static str>,
) -> (
    SharedBatchIngest,
    Arc<SqlMemoryService>,
    Arc<SqlKnowledgeStore>,
    Arc<AtomicUsize>,
    BatchIngestRequest,
) {
    let scope: Scope =
        serde_json::from_value(json!({"tenant":"contract","workspace":"workspace"})).unwrap();
    let memory = Arc::new(SqlMemoryService::open_in_memory().unwrap());
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let entity:KnowledgeEntity=serde_json::from_value(json!({"id":"later-entity","kind":"concept","name":"later","scope":scope,"createdAt":"2026-11-17T00:00:00Z","provenance":{"source":"contract","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"workspace","retention":"durable"}})).unwrap();
    let request = BatchIngestRequest {
        idempotency_key: "batch".into(),
        scope: scope.clone(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![
            fact("early", "early success", &scope),
            fact("bad", "", &scope),
            fact("unknown", "landed unknown", &scope),
            fact("later", "must not send", &scope),
        ],
        entities: vec![entity],
        relationships: Vec::new(),
        evidence: Vec::new(),
        embeddings: Vec::new(),
        source_id: None,
    };
    let ports = CallPorts {
        memory: memory.clone(),
        knowledge: knowledge.clone(),
    };
    let factory = Factory {
        ports,
        scope,
        calls: calls.clone(),
        unknown_at,
        blocked_at,
        mismatch,
    };
    (
        SharedBatchIngest::new(Arc::new(factory)),
        memory,
        knowledge,
        calls,
        request,
    )
}
fn setup(
    unknown_at: usize,
) -> (
    SharedBatchIngest,
    Arc<SqlMemoryService>,
    Arc<SqlKnowledgeStore>,
    Arc<AtomicUsize>,
    BatchIngestRequest,
) {
    setup_mode(unknown_at, 0, None)
}
#[test]
fn typed_unknown_stops_later_mutations_and_outranks_prior_definite_failure() {
    block_on(async {
        let (batch, memory, knowledge, calls, request) = setup(3);
        let scope = request.scope.clone();
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(
            memory.list_memories_in_scope(&scope).unwrap().len(),
            2,
            "unknown result must stop later writes, retaining only earlier success and the landed uncertain write"
        );
        assert!(knowledge.list_entities(&scope).await.unwrap().is_empty());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "no later port admission after exact unknown"
        );
        let facts = result
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Facts)
            .unwrap();
        assert_eq!(facts.status, StepStatus::Failed);
        assert!(
            facts
                .error
                .as_ref()
                .unwrap()
                .to_redacted()
                .to_string()
                .contains(ORIGINAL)
        );
        assert_eq!(
            result
                .steps
                .iter()
                .find(|s| s.step == BatchStep::Entities)
                .unwrap()
                .status,
            StepStatus::Skipped
        );
    });
}
#[test]
fn definite_failure_preserves_best_effort_continuation() {
    block_on(async {
        let (batch, memory, knowledge, _, request) = setup(usize::MAX);
        let scope = request.scope.clone();
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(memory.list_memories_in_scope(&scope).unwrap().len(), 3);
        assert_eq!(knowledge.list_entities(&scope).await.unwrap().len(), 1);
        assert_eq!(
            result
                .steps
                .iter()
                .find(|s| s.step == BatchStep::Facts)
                .unwrap()
                .status,
            StepStatus::Failed
        );
        assert_eq!(
            result
                .steps
                .iter()
                .find(|s| s.step == BatchStep::Entities)
                .unwrap()
                .status,
            StepStatus::Succeeded
        );
    });
}

#[test]
fn knowledge_unknown_stops_later_items_and_steps() {
    block_on(async {
        let (batch, memory, knowledge, calls, mut request) = setup(2);
        let scope = request.scope.clone();
        request.facts.clear();
        let base = request.entities[0].clone();
        request.entities = (0..3)
            .map(|i| {
                let mut e = base.clone();
                e.id = format!("knowledge-{i}").into();
                e
            })
            .collect();
        let result = batch.ingest(request).await.unwrap();
        assert!(memory.list_memories_in_scope(&scope).unwrap().is_empty());
        assert_eq!(knowledge.list_entities(&scope).await.unwrap().len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let step = result
            .steps
            .iter()
            .find(|s| s.step == BatchStep::Entities)
            .unwrap();
        assert_eq!(step.status, StepStatus::Failed);
        assert!(
            step.error
                .as_ref()
                .unwrap()
                .to_redacted()
                .to_string()
                .contains(ORIGINAL)
        );
    });
}
#[test]
fn blocked_ledger_read_preserves_episode_success_without_new_effect() {
    block_on(async {
        let (batch, memory, knowledge, calls, mut request) = setup_mode(usize::MAX, 3, None);
        let scope = request.scope.clone();
        request.source_id = Some("source-A".into());
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(knowledge.list_sources(&scope).await.unwrap().len(), 1);
        assert!(memory.list_memories_in_scope(&scope).unwrap().is_empty());
        assert!(knowledge.list_entities(&scope).await.unwrap().is_empty());
        assert_eq!(result.steps[0].status, StepStatus::Succeeded);
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert!(
            result.steps[1]
                .error
                .as_ref()
                .unwrap()
                .to_redacted()
                .to_string()
                .contains(ORIGINAL)
        );
        assert_eq!(result.steps[2].status, StepStatus::Skipped);
    });
}
#[test]
fn ledger_unknown_keeps_landed_fact_and_stops_following_step() {
    block_on(async {
        use engram_knowledge::SourceBatchLedger;
        let (batch, memory, knowledge, calls, mut request) = setup(5);
        let scope = request.scope.clone();
        request.source_id = Some("source-A".into());
        request.facts.truncate(1);
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        assert_eq!(memory.list_memories_in_scope(&scope).unwrap().len(), 1);
        assert_eq!(
            knowledge
                .list_source_batch_records(&scope, "source-A")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(knowledge.list_entities(&scope).await.unwrap().is_empty());
        assert_eq!(result.steps[0].status, StepStatus::Succeeded);
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert_eq!(result.steps[2].status, StepStatus::Skipped);
    });
}
#[test]
fn supersession_unknown_leaves_prior_ledger_unsuperseded_and_no_replacement() {
    block_on(async {
        use engram_knowledge::SourceBatchLedger;
        let (batch, memory, knowledge, calls, mut request) = setup(5);
        let scope = request.scope.clone();
        request.source_id = Some("source-A".into());
        request.facts.truncate(1);
        request.idempotency_key = "revision".into();
        let mut seed = request.clone();
        seed.idempotency_key = "seed".into();
        seed.entities.clear();
        let seed_batch =
            engram_integration::sqlite::SqlBatchIngest::new(memory.clone(), knowledge.clone());
        seed_batch.ingest(seed).await.unwrap();
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        let rows = memory.list_memories_in_scope(&scope).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, MemoryStatus::Forgotten);
        let ledger = knowledge
            .list_source_batch_records(&scope, "source-A")
            .await
            .unwrap();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].batch_key, "seed");
        assert!(ledger[0].superseded_at.is_none());
        assert!(knowledge.list_entities(&scope).await.unwrap().is_empty());
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert!(
            result.steps[1]
                .error
                .as_ref()
                .unwrap()
                .to_redacted()
                .to_string()
                .contains(ORIGINAL)
        );
        assert_eq!(result.steps[2].status, StepStatus::Skipped);
    });
}
#[test]
fn foreign_session_call_dimensions_reject_before_mutation() {
    block_on(async {
        for dimension in [
            "backend", "mode", "store", "session", "actor", "item", "scope",
        ] {
            let (batch, memory, knowledge, calls, request) =
                setup_mode(usize::MAX, 0, Some(dimension));
            let scope = request.scope.clone();
            let result = batch.ingest(request).await.unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(memory.list_memories_in_scope(&scope).unwrap().is_empty());
            assert!(knowledge.list_entities(&scope).await.unwrap().is_empty());
            assert_eq!(result.steps[1].status, StepStatus::Failed);
            assert_eq!(result.steps[2].status, StepStatus::Skipped);
        }
    });
}
#[test]
fn mismatched_completed_call_halts_without_fabricating_an_operation_id() {
    block_on(async {
        let (batch, memory, _, calls, request) =
            setup_mode(usize::MAX, 0, Some("observation-call"));
        let scope = request.scope.clone();
        let result = batch.ingest(request).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            memory.list_memories_in_scope(&scope).unwrap().len(),
            1,
            "already-landed effect is retained"
        );
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert!(
            !result.steps[1]
                .error
                .as_ref()
                .unwrap()
                .to_string()
                .contains(ORIGINAL)
        );
        assert_eq!(result.steps[2].status, StepStatus::Skipped);
    });
}
#[test]
fn complete_scope_reservation_distinguishes_unset_and_empty() {
    let (_, memory, knowledge, _, request) = setup(usize::MAX);
    let scope = request.scope;
    let storage = PortStorageComposition::new(
        CallPorts { memory, knowledge },
        "sqlite",
        "local",
        "same-owned-store",
    );
    let first = storage.session(&scope).unwrap();
    assert!(storage.session(&scope).is_err());
    let mut sibling = scope.clone();
    sibling.environment = Some(String::new());
    let second = storage.session(&sibling).unwrap();
    assert_ne!(first.binding().scope, second.binding().scope);
    drop(first);
    assert!(storage.session(&scope).is_ok());
}

fn episode_source(id: &str, scope: &Scope) -> KnowledgeSource {
    KnowledgeSource {
        id: id.into(),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: id.into(),
        uri: None,
        version: None,
        repository_key: None,
        git_ref: None,
        policy: fact("template", "text", scope).policy,
        provenance: fact("template", "text", scope).provenance,
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}
fn episode_document(id: &str, source: &str, title: &str) -> SourceDocument {
    SourceDocument {
        id: id.into(),
        source_id: source.into(),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some(format!("docs/{id}.md")),
        title: Some(title.into()),
        mime_type: None,
        language: None,
        version: None,
        content_hash: format!("hash-{title}"),
        provenance: fact(
            "template",
            "text",
            &Scope {
                tenant: "fixture".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
        )
        .provenance,
        policy: fact(
            "template",
            "text",
            &Scope {
                tenant: "fixture".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
        )
        .policy,
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}
fn episode_chunk(id: &str, doc: &str, source: &str, text: &str) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.into(),
        document_id: doc.into(),
        source_id: source.into(),
        kind: KnowledgeChunkKind::Paragraph,
        text: text.into(),
        summary: None,
        location: None,
        entities: vec![],
        concepts: vec![],
        embedding_refs: vec![],
        content_hash: format!("hash-{text}"),
        provenance: episode_document("t", "t", "t").provenance,
        policy: episode_document("t", "t", "t").policy,
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}
fn local_composition(
    memory: Arc<SqlMemoryService>,
    knowledge: Arc<SqlKnowledgeStore>,
) -> PortStorageComposition {
    PortStorageComposition::new(
        CallPorts { memory, knowledge },
        "sqlite",
        "local",
        "owned-r2",
    )
}
#[test]
fn malformed_or_contradictory_causal_evidence_never_publishes_private_or_foreign_ids() {
    block_on(async {
        use engram_memory::MemoryEventRepository;
        for mismatch in [
            "unknown-malformed",
            "unknown-foreign-commit",
            "unknown-missing-step",
            "unknown-empty-step",
            "unknown-missing-commit",
            "unknown-uppercase",
            "unknown-wrong-version",
            "cancelled-with-commit",
            "blocked-with-commit",
            "blocked-with-step",
            "definite-malformed-commit",
            "ack-without-step",
            "staged-malformed-commit",
        ] {
            let (_, memory, knowledge, calls, request) = setup_mode(1, 0, Some(mismatch));
            let observed = ObservedPorts::new(Arc::new(Session {
                ports: CallPorts { memory, knowledge },
                scope: request.scope.clone(),
                calls: calls.clone(),
                unknown_at: 1,
                blocked_at: 0,
                mismatch: Some(mismatch),
            }));
            let public = observed
                .list_events_for_scope(&request.scope)
                .await
                .unwrap_err()
                .to_redacted()
                .to_string();
            for private in [
                "credential-secret",
                "private-record",
                "/owned/path",
                ORIGINAL,
                "0152c084-4ca6-431c-84f2-227a03abf5a5",
            ] {
                assert!(!public.contains(private), "{mismatch}: {public}");
            }
            let stopped = observed.state.lock().unwrap().stopped.clone().unwrap();
            assert_eq!(stopped.cleanup, CleanupCertainty::Unconfirmed, "{mismatch}");
            assert_eq!(stopped.outcome, EffectOutcome::DefiniteFailure);
            assert_eq!(stopped.commit_id, None);
            assert!(
                observed
                    .list_events_for_scope(&request.scope)
                    .await
                    .is_err()
            );
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    });
}
#[test]
fn actual_request_and_record_scopes_reject_before_call_admission() {
    block_on(async {
        use engram_knowledge::{KnowledgeRepository, SourceBatchLedger};
        use engram_memory::{MemoryEventRepository, MemoryRepository, MemoryService};
        let (_, memory, knowledge, calls, request) = setup(usize::MAX);
        let scope = request.scope.clone();
        let mut foreign = scope.clone();
        foreign.session = Some(String::new());
        let observed = ObservedPorts::new(Arc::new(Session {
            ports: CallPorts {
                memory: memory.clone(),
                knowledge: knowledge.clone(),
            },
            scope: scope.clone(),
            calls: calls.clone(),
            unknown_at: usize::MAX,
            blocked_at: 0,
            mismatch: None,
        }));
        assert!(observed.put_memory(fact("x", "x", &foreign)).await.is_err());
        assert!(observed.get_memory(&"x".into(), &foreign).await.is_err());
        assert!(observed.list_events_for_scope(&foreign).await.is_err());
        assert!(
            observed
                .put_source(episode_source("foreign", &foreign))
                .await
                .is_err()
        );
        assert!(
            observed
                .get_source(&"foreign".into(), &foreign)
                .await
                .is_err()
        );
        let mut entity = request.entities[0].clone();
        entity.scope = foreign.clone();
        assert!(observed.put_entity(entity).await.is_err());
        assert!(
            observed
                .list_source_batch_records(&foreign, "source")
                .await
                .is_err()
        );
        assert!(
            observed
                .record_source_batch_writes(
                    &foreign,
                    "source",
                    "batch",
                    engram_knowledge::SourceBatchRecordKind::MemoryRecord,
                    &[]
                )
                .await
                .is_err()
        );
        let mut retrieval:RetrievalRequest=serde_json::from_value(json!({"query":"x","scope":foreign,"requester":{"actor":{"id":"actor","kind":"agent"}}})).unwrap();
        retrieval.scope = foreign;
        assert!(observed.retrieve(retrieval).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(memory.list_memories_in_scope(&scope).unwrap().is_empty());
        assert!(knowledge.list_sources(&scope).await.unwrap().is_empty());
    });
}
#[test]
fn full_scope_episode_parents_and_existing_owners_are_checked_before_writes() {
    block_on(async {
        use engram_knowledge::KnowledgeRepository;
        for dimension in ["tenant", "subject", "workspace", "session", "environment"] {
            let (_, memory, knowledge, _, mut request) = setup(usize::MAX);
            let mut a = request.scope.clone();
            a.subject = None;
            a.workspace = None;
            a.session = None;
            a.environment = None;
            let mut b = a.clone();
            match dimension {
                "tenant" => b.tenant = "foreign".into(),
                "subject" => b.subject = Some(String::new()),
                "workspace" => b.workspace = Some(String::new()),
                "session" => b.session = Some(String::new()),
                _ => b.environment = Some(String::new()),
            }
            knowledge
                .put_source(episode_source("source-a", &a))
                .await
                .unwrap();
            knowledge
                .put_source(episode_source("source-b", &b))
                .await
                .unwrap();
            knowledge
                .put_document(episode_document("doc-a", "source-a", "original-a"))
                .await
                .unwrap();
            knowledge
                .put_document(episode_document("doc-b", "source-b", "original-b"))
                .await
                .unwrap();
            knowledge
                .put_chunk(episode_chunk("chunk-a", "doc-a", "source-a", "original-a"))
                .await
                .unwrap();
            knowledge
                .put_chunk(episode_chunk("chunk-b", "doc-b", "source-b", "original-b"))
                .await
                .unwrap();
            let batch =
                SharedBatchIngest::new(Arc::new(local_composition(memory, knowledge.clone())));
            request.scope = a.clone();
            request.facts.clear();
            request.entities.clear();
            request.documents = vec![episode_document("doc-a", "source-a", "allowed")];
            request.chunks = vec![episode_chunk("chunk-a", "doc-a", "source-a", "allowed")];
            assert_eq!(
                batch.ingest(request.clone()).await.unwrap().steps[0].status,
                StepStatus::Succeeded
            );
            request.documents = vec![episode_document("doc-b", "source-b", "foreign-overwrite")];
            request.chunks = vec![episode_chunk(
                "chunk-b",
                "doc-b",
                "source-b",
                "foreign-overwrite",
            )];
            assert_eq!(
                batch.ingest(request.clone()).await.unwrap().steps[0].status,
                StepStatus::Failed
            );
            request.documents = vec![episode_document("doc-b", "source-a", "reparent-overwrite")];
            request.chunks = vec![episode_chunk(
                "chunk-b",
                "doc-a",
                "source-a",
                "reparent-overwrite",
            )];
            assert_eq!(
                batch.ingest(request.clone()).await.unwrap().steps[0].status,
                StepStatus::Failed
            );
            assert_eq!(
                knowledge
                    .list_documents(&b)
                    .await
                    .unwrap()
                    .into_iter()
                    .find(|d| d.id.as_str() == "doc-b")
                    .unwrap()
                    .title
                    .as_deref(),
                Some("original-b")
            );
            assert_eq!(
                knowledge
                    .get_chunk(&"chunk-b".into(), &b)
                    .await
                    .unwrap()
                    .unwrap()
                    .text,
                "original-b"
            );
            assert!(
                knowledge
                    .put_source(episode_source("source-b", &a))
                    .await
                    .is_err()
            );
            assert!(
                knowledge
                    .put_document_in_scope(
                        &a,
                        episode_document("missing-source-doc", "absent", "x")
                    )
                    .await
                    .is_err()
            );
            assert!(
                knowledge
                    .put_chunk_in_scope(
                        &a,
                        episode_chunk("missing-document", "absent", "source-a", "x")
                    )
                    .await
                    .is_err()
            );
            assert!(
                knowledge
                    .put_chunk_in_scope(
                        &a,
                        episode_chunk("contradictory-parent", "doc-b", "source-a", "x")
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                knowledge
                    .list_documents(&a)
                    .await
                    .unwrap()
                    .into_iter()
                    .find(|d| d.id.as_str() == "doc-a")
                    .unwrap()
                    .title
                    .as_deref(),
                Some("allowed")
            );
        }
    });
}

#[test]
fn contributor_checks_expected_scope_against_current_parent_and_immutable_inline_owners() {
    block_on(async {
        use engram_knowledge::KnowledgeRepository;
        let (_, memory, knowledge, _, request) = setup(usize::MAX);
        let a = request.scope.clone();
        let mut b = a.clone();
        b.environment = Some(String::new());
        knowledge
            .put_source(episode_source("recreated-source", &a))
            .await
            .unwrap();
        let composition = local_composition(memory, knowledge.clone());
        let observed = ObservedPorts::new(composition.session(&a).unwrap());
        // A raw owner changes the parent after the session was acquired. The actual
        // scoped contributor rechecks it while holding the write lock.
        knowledge
            .shared_connection()
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM knowledge_sources WHERE id = ?1",
                ["recreated-source"],
            )
            .unwrap();
        knowledge
            .put_source(episode_source("recreated-source", &b))
            .await
            .unwrap();
        assert!(
            observed
                .put_document(episode_document("must-not-land", "recreated-source", "x"))
                .await
                .is_err()
        );
        assert!(knowledge.list_documents(&b).await.unwrap().is_empty());
        let mut entity = request.entities[0].clone();
        entity.scope = b.clone();
        knowledge.put_entity(entity.clone()).await.unwrap();
        let original = entity.clone();
        entity.scope = a.clone();
        assert!(observed.put_entity(entity).await.is_err());
        assert_eq!(
            knowledge.get_entity(&original.id, &b).await.unwrap(),
            Some(original)
        );
        let mut edge:KnowledgeRelationship=serde_json::from_value(json!({"id":"foreign-edge","subject":{"id":"left"},"predicate":"links","object":{"id":"right"},"scope":b,"createdAt":"2026-11-17T00:00:00Z","provenance":fact("template","text",&b).provenance})).unwrap();
        knowledge.put_relationship(edge.clone()).await.unwrap();
        let original = edge.clone();
        edge.scope = a;
        assert!(observed.put_relationship(edge).await.is_err());
        assert_eq!(
            knowledge.get_relationship(&original.id, &b).await.unwrap(),
            Some(original)
        );
    });
}

#[test]
fn scoped_entity_guard_preserves_temporal_archive_and_change_log_noop_behavior() {
    block_on(async {
        use engram_knowledge::KnowledgeRepository;
        let (_, _, knowledge, _, request) = setup(usize::MAX);
        let scope = request.scope;
        let mut entity = request.entities[0].clone();
        let original_time: Timestamp = "2026-11-01T00:00:00Z".parse().unwrap();
        entity.valid_from = Some(original_time);
        entity.archived_at = Some(original_time);
        let stored = knowledge
            .put_entity_in_scope(&scope, entity.clone())
            .await
            .unwrap();
        entity.valid_from = Some("2026-11-17T00:00:00Z".parse().unwrap());
        entity.archived_at = None;
        let repeated = knowledge.put_entity_in_scope(&scope, entity).await.unwrap();
        assert_eq!(repeated, stored);
        let connection = knowledge.shared_connection();
        let count: i64 = connection
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM knowledge_change_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 1,
            "guarded no-op re-put retains the existing change-log rule"
        );
    });
}

#[test]
fn same_scope_document_reassociation_allows_matching_chunk_repair_in_both_orders() {
    block_on(async {
        use engram_knowledge::KnowledgeRepository;
        for child_first in [false, true] {
            let (_, _, knowledge, _, request) = setup(usize::MAX);
            let scope = request.scope;
            knowledge
                .put_source(episode_source("one", &scope))
                .await
                .unwrap();
            knowledge
                .put_source(episode_source("two", &scope))
                .await
                .unwrap();
            knowledge
                .put_document(episode_document("document", "one", "original"))
                .await
                .unwrap();
            knowledge
                .put_chunk(episode_chunk("chunk", "document", "one", "original"))
                .await
                .unwrap();
            if child_first {
                // Incoming mismatched parents still fail before changing the old child.
                assert!(
                    knowledge
                        .put_chunk(episode_chunk("chunk", "document", "two", "premature"))
                        .await
                        .is_err()
                );
                assert_eq!(
                    knowledge
                        .get_chunk(&"chunk".into(), &scope)
                        .await
                        .unwrap()
                        .unwrap()
                        .source_id
                        .as_str(),
                    "one"
                );
            }
            knowledge
                .put_document(episode_document("document", "two", "acknowledged"))
                .await
                .unwrap();
            knowledge
                .put_chunk(episode_chunk("chunk", "document", "two", "repaired"))
                .await
                .unwrap();
            let document = knowledge
                .list_documents(&scope)
                .await
                .unwrap()
                .pop()
                .unwrap();
            let chunk = knowledge
                .get_chunk(&"chunk".into(), &scope)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(document.source_id, chunk.source_id);
            assert_eq!(chunk.text, "repaired");
        }
    });
}

#[test]
fn same_scope_chunk_can_move_to_an_existing_valid_document_before_old_parent_reassociation() {
    block_on(async {
        use engram_knowledge::KnowledgeRepository;
        let (_, _, knowledge, _, request) = setup(usize::MAX);
        let scope = request.scope;
        knowledge
            .put_source(episode_source("one", &scope))
            .await
            .unwrap();
        knowledge
            .put_source(episode_source("two", &scope))
            .await
            .unwrap();
        knowledge
            .put_document(episode_document("old-document", "one", "old"))
            .await
            .unwrap();
        knowledge
            .put_document(episode_document("new-document", "two", "new"))
            .await
            .unwrap();
        knowledge
            .put_chunk(episode_chunk("chunk", "old-document", "one", "original"))
            .await
            .unwrap();
        knowledge
            .put_chunk(episode_chunk("chunk", "new-document", "two", "moved-first"))
            .await
            .unwrap();
        knowledge
            .put_document(episode_document("old-document", "two", "moved-after"))
            .await
            .unwrap();
        let chunk = knowledge
            .get_chunk(&"chunk".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(chunk.document_id.as_str(), "new-document");
        assert_eq!(chunk.source_id.as_str(), "two");
    });
}
