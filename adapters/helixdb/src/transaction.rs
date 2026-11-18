//! One storage transaction per acknowledged step; earlier steps remain durable.
use crate::{
    HelixStore,
    error::{Error, Result},
    records::{self, EDGE, ENDPOINT, RELATIONSHIP},
    transport::Operation,
};
use engram_runtime::effect::{CleanupCertainty, EffectFrame, EffectIdentity, EffectOutcome};
use std::sync::{Arc, Mutex};
pub(crate) type StepFrames = Arc<Mutex<Vec<(EffectIdentity, EffectFrame)>>>;
/// One canonical receipt-response predicate, reused by nonproduction observers.
pub(crate) fn acknowledged_receipt(
    response: &Value,
    receipt: &Value,
    operation_id: &str,
    partition: &str,
    mutation_guard: &str,
) -> bool {
    let owners = response.get("owner").and_then(Value::as_array);
    response
        .get("receipt")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            response
                .get(mutation_guard)
                .and_then(Value::as_array)
                .is_some_and(|v| v.len() == 1)
                && owners.is_some_and(|owners| owners.len() == 1)
                && rows.len() == 1
                && rows[0].get("public_id").and_then(Value::as_str) == Some(operation_id)
                && rows[0].get("partition").and_then(Value::as_str) == Some(partition)
                && rows[0]
                    .get("payload")
                    .and_then(Value::as_str)
                    .and_then(|v| serde_json::from_str::<Value>(v).ok())
                    .as_ref()
                    == Some(receipt)
        })
}
#[cfg(all(test, feature = "test-controls"))]
mod receipt_predicate_controls {
    use super::*;
    #[test]
    fn canonical_predicate_rejects_forged_owner_partition_id_payload_and_guard() {
        let id = "11111111-2222-4333-8444-555555555555";
        let expected =
            serde_json::json!({"owner":"actual-owner","epoch":1,"scope":{"tenant":"owned"}});
        let response = serde_json::json!({"owner":[{}],"guard":[{}],"receipt":[{"public_id":id,"partition":"actual-partition","payload":expected.to_string()}]});
        assert!(acknowledged_receipt(
            &response,
            &expected,
            id,
            "actual-partition",
            "guard"
        ));
        for field in ["public_id", "partition", "payload"] {
            let mut forged = response.clone();
            forged["receipt"][0][field] = serde_json::json!("foreign");
            assert!(!acknowledged_receipt(
                &forged,
                &expected,
                id,
                "actual-partition",
                "guard"
            ));
        }
        for field in ["owner", "guard"] {
            let mut forged = response.clone();
            forged[field] = serde_json::json!([]);
            assert!(!acknowledged_receipt(
                &forged,
                &expected,
                id,
                "actual-partition",
                "guard"
            ));
        }
        let mut foreign = expected.clone();
        foreign["owner"] = serde_json::json!("foreign-owner");
        let mut forged = response.clone();
        forged["receipt"][0]["payload"] = serde_json::json!(foreign.to_string());
        assert!(!acknowledged_receipt(
            &forged,
            &expected,
            id,
            "actual-partition",
            "guard"
        ));
    }
}
use helix_db::dsl::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, serde::Serialize)]
pub(crate) struct Fence {
    pub(crate) family: String,
    pub(crate) predicate: SourcePredicate,
    pub(crate) minimum: usize,
    pub(crate) empty: bool,
    pub(crate) reason: &'static str,
}
pub(crate) struct StepState {
    pub(crate) frames: StepFrames,
    pub(crate) fences: Vec<Fence>,
    pub(crate) id: String,
    pub(crate) mutations: BTreeMap<(String, String), Option<Value>>,
}
/// Explicit step boundary. Drop abandons staged work only.
pub struct StorageStep {
    pub(crate) id: String,
    store: HelixStore,
    finished: bool,
    pub(crate) frames: StepFrames,
    sent_operation: Option<String>,
}
impl Drop for StorageStep {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(frames) = self.frames.lock() {
                for (identity, frame) in frames.iter() {
                    if frame.is_pending(identity).unwrap_or(false) {
                        if let Some(operation_id) = &self.sent_operation {
                            let _ = frame.finish(
                                identity,
                                &self.id,
                                EffectOutcome::Unknown {
                                    operation_id: operation_id.clone(),
                                },
                                CleanupCertainty::Unconfirmed,
                            );
                        } else {
                            let _ = frame.cancel(identity, &self.id);
                        }
                    }
                }
            }
            if let Some(s) = &self.store.session {
                if let Ok(mut p) = s.pending.lock() {
                    if p.as_ref().is_some_and(|step| step.id == self.id) {
                        *p = None;
                    }
                }
            }
        }
    }
}
impl HelixStore {
    pub fn begin_step(&self) -> Result<StorageStep> {
        self.admit_logical_work()?;
        let mut caller = self.clone();
        let _call = caller.enter_call()?;
        let state = self
            .session
            .as_ref()
            .ok_or(Error::InvalidConfig("begin a scope session before a step"))?;
        let mut pending = state.pending.lock().map_err(|_| Error::Storage)?;
        if pending.is_some() {
            return Err(Error::Contention);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let frames = Arc::new(Mutex::new(Vec::new()));
        *pending = Some(StepState {
            frames: frames.clone(),
            id: id.clone(),
            mutations: BTreeMap::new(),
            fences: Vec::new(),
        });
        let mut store = self.clone();
        store.step_id = Some(id.clone());
        Ok(StorageStep {
            id,
            store,
            finished: false,
            frames,
            sent_operation: None,
        })
    }
}
impl StorageStep {
    fn observe_result(&self, operation_id: &str, error: Option<&Error>) -> Result<()> {
        let frames = self.frames.lock().map_err(|_| Error::Storage)?;
        for (identity, frame) in frames.iter() {
            if !frame
                .is_pending(identity)
                .map_err(|_| Error::CleanupUnconfirmed)?
            {
                continue;
            }
            let outcome = match error {
                None => EffectOutcome::Acknowledged,
                Some(error) => match error.outcome_unknown_operation() {
                    Some(actual) if actual == operation_id => EffectOutcome::Unknown {
                        operation_id: actual.into(),
                    },
                    Some(_) => return Err(Error::CleanupUnconfirmed),
                    None => EffectOutcome::DefiniteFailure,
                },
            };
            let cleanup = if error.is_some_and(|e| {
                matches!(
                    e,
                    Error::CleanupAfterFailure { .. } | Error::CleanupUnconfirmed
                )
            }) {
                CleanupCertainty::Unconfirmed
            } else {
                CleanupCertainty::Confirmed
            };
            frame
                .finish(identity, &self.id, outcome, cleanup)
                .map_err(|_| Error::CleanupUnconfirmed)?;
            #[cfg(feature = "test-controls")]
            if let Ok(observation) = frame.observation(identity) {
                self.store.inner.transport.fixture_observed(observation);
            }
        }
        Ok(())
    }
    pub fn commit(mut self) -> Result<String> {
        self.store.admit_logical_work()?;
        let _call = self.store.enter_call()?;
        let state = self.store.session.as_ref().ok_or(Error::Storage)?;
        let mut pending = state.pending.lock().map_err(|_| Error::Storage)?;
        let step = pending
            .as_mut()
            .filter(|step| step.id == self.id)
            .ok_or(Error::Contention)?;
        self.store.retain_step_effect(&self.id, &self.frames)?;
        let explicit_mutation_count = step.mutations.len();
        crate::changes::augment(&self.store, &mut step.mutations)?;
        let operation_id = uuid::Uuid::new_v4().to_string();
        let lease = self.store.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
        let partition = self.store.inner.partition.discriminator()?;
        let mut batch = lease.guard();
        let mut mutation_guard = "owner".to_owned();
        let mut checks = Vec::new();
        for (i, fence) in step.fences.iter().enumerate() {
            let check = format!("check{i}");
            let next = format!("fence{i}");
            batch = batch.var_as(
                &check,
                g().n_with_label_where(
                    &fence.family,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq("partition", partition.clone()),
                        fence.predicate.clone(),
                    ]),
                ),
            );
            let condition = if fence.empty {
                BatchCondition::VarEmpty(check.clone())
            } else {
                BatchCondition::VarMinSize(check.clone(), fence.minimum)
            };
            batch = batch.var_as_if(&next, condition, g().n(NodeRef::var(&mutation_guard)));
            checks.push(check);
            mutation_guard = next;
        }
        for (i, ((family, id), payload)) in step.mutations.iter().enumerate() {
            let existing = format!("existing{i}");
            let removed = format!("removed{i}");
            let created = format!("created{i}");
            batch = batch.var_as(
                &existing,
                g().n_with_label_where(
                    family,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq(
                            "key",
                            self.store.inner.partition.record_key(family, id)?,
                        ),
                        SourcePredicate::eq("partition", partition.clone()),
                    ]),
                ),
            );
            batch = batch.var_as_if(
                &removed,
                BatchCondition::VarMinSize(mutation_guard.clone(), 1),
                g().n(NodeRef::var(&existing)).drop(),
            );
            if family == RELATIONSHIP {
                let old = format!("oldedge{i}");
                batch = batch.var_as_if(
                    &old,
                    BatchCondition::VarMinSize(mutation_guard.clone(), 1),
                    g().e_with_label_where(
                        EDGE,
                        SourcePredicate::and(vec![
                            SourcePredicate::eq("partition", partition.clone()),
                            SourcePredicate::eq("relationship_id", id.clone()),
                        ]),
                    )
                    .drop(),
                );
            }
            if let Some(payload) = payload {
                batch = batch.var_as_if(
                    &created,
                    BatchCondition::VarMinSize(mutation_guard.clone(), 1),
                    g().add_n(
                        family,
                        records::properties(&self.store, family, id, payload)?,
                    ),
                );
            }
        }
        for (i, ((family, id), payload)) in step.mutations.iter().enumerate() {
            if family != RELATIONSHIP {
                continue;
            }
            if let Some(v) = payload {
                let subject = v
                    .get("_subject_endpoint")
                    .and_then(Value::as_str)
                    .ok_or(Error::Storage)?;
                let object = v
                    .get("_object_endpoint")
                    .and_then(Value::as_str)
                    .ok_or(Error::Storage)?;
                let from = format!("from{i}");
                let to = format!("to{i}");
                for (name, endpoint) in [(&from, subject), (&to, object)] {
                    batch = batch.var_as(
                        name,
                        g().n_with_label_where(
                            ENDPOINT,
                            SourcePredicate::and(vec![
                                SourcePredicate::eq("partition", partition.clone()),
                                SourcePredicate::eq("public_id", endpoint),
                            ]),
                        ),
                    );
                }
                batch = batch.var_as_if(
                    &format!("edge{i}"),
                    BatchCondition::VarMinSize(mutation_guard.clone(), 1),
                    g().n(NodeRef::var(&from)).add_e(
                        EDGE,
                        NodeRef::var(&to),
                        vec![
                            ("partition", PropertyInput::from(partition.clone())),
                            ("relationship_id", id.clone().into()),
                            (
                                "graphId",
                                v.get("graphId")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .into(),
                            ),
                            (
                                "subject_id",
                                v.pointer("/subject/id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .into(),
                            ),
                            (
                                "object_id",
                                v.pointer("/object/id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .into(),
                            ),
                        ],
                    ),
                );
            }
        }
        let mut receipt =
            crate::receipts::receipt(&self.store, &operation_id, &step.mutations, &step.fences)?;
        // The fingerprint still covers every physical effect; preserve caller staging count.
        receipt["mutation_count"] = explicit_mutation_count.into();
        receipt["auxiliary_mutation_count"] = step
            .mutations
            .len()
            .checked_sub(explicit_mutation_count)
            .ok_or(Error::Storage)?
            .into();
        batch = batch
            .var_as_if(
                "receipt",
                BatchCondition::VarMinSize(mutation_guard.clone(), 1),
                g().add_n(
                    records::RECEIPT,
                    records::properties(&self.store, records::RECEIPT, &operation_id, &receipt)?,
                )
                .value_map(Some(vec!["public_id", "partition", "payload"])),
            )
            .returning(
                [
                    {
                        let mut names = vec!["owner".to_owned(), "receipt".to_owned()];
                        if mutation_guard != "owner" {
                            names.push(mutation_guard.clone());
                        }
                        names
                    },
                    checks.clone(),
                ]
                .concat(),
            );
        let operation = Operation::with_id(QueryRequest::write(batch), operation_id.clone())?;
        // Bind the actual generated operation UUID before the transport can send.
        {
            let frames = self.frames.lock().map_err(|_| Error::Storage)?;
            for (identity, frame) in frames.iter() {
                if frame.is_pending(identity).map_err(|_| Error::Contention)? {
                    frame
                        .bind_commit(identity, &self.id, &operation_id)
                        .map_err(|_| Error::Contention)?;
                }
            }
        }
        self.store
            .inner
            .uncertain_receipts
            .lock()
            .map_err(|_| Error::Storage)?
            .insert(
                operation_id.clone(),
                crate::receipts::PendingReceipt {
                    expected: receipt.clone(),
                    transport_unknown: false,
                },
            );
        self.sent_operation = Some(operation_id.clone());
        #[cfg(feature = "test-controls")]
        if let Ok(frames) = self.frames.lock() {
            let expected = receipt.clone();
            let actual_id = operation_id.clone();
            let actual_partition = partition.clone();
            let actual_guard = mutation_guard.clone();
            self.store.inner.transport.fixture_bind_effects(
                &operation_id,
                frames
                    .iter()
                    .filter_map(|(identity, frame)| frame.observation(identity).ok())
                    .collect(),
                crate::transport::receipt_hash(&receipt),
                serde_json::json!({"partition":partition,"mutationGuard":mutation_guard}),
                Arc::new(move |response| {
                    acknowledged_receipt(
                        response,
                        &expected,
                        &actual_id,
                        &actual_partition,
                        &actual_guard,
                    )
                }),
            );
        }
        let response = match self.store.inner.transport.execute(&operation) {
            Ok(response) => response,
            Err(error) => {
                if error.outcome_unknown_operation().is_some() {
                    if let Some(pending) = self
                        .store
                        .inner
                        .uncertain_receipts
                        .lock()
                        .map_err(|_| Error::Storage)?
                        .get_mut(&operation_id)
                    {
                        pending.transport_unknown = true;
                    }
                    self.store
                        .inner
                        .cleanup_only
                        .store(true, std::sync::atomic::Ordering::Release);
                } else {
                    self.store
                        .inner
                        .uncertain_receipts
                        .lock()
                        .map_err(|_| Error::Storage)?
                        .remove(&operation_id);
                }
                self.observe_result(&operation_id, Some(&error))?;
                return Err(error);
            }
        };
        let rows = response.get("receipt").and_then(Value::as_array);
        if response
            .get("owner")
            .and_then(Value::as_array)
            .is_some_and(|v| v.len() == 1)
            && rows.is_some_and(Vec::is_empty)
            && response
                .get(&mutation_guard)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        {
            for (fence, name) in step.fences.iter().zip(checks.iter()) {
                if let Some(values) = response.get(name).and_then(Value::as_array) {
                    let matched = if fence.empty {
                        values.is_empty()
                    } else {
                        values.len() >= fence.minimum
                    };
                    if !matched {
                        self.store
                            .inner
                            .uncertain_receipts
                            .lock()
                            .map_err(|_| Error::Storage)?
                            .remove(&operation_id);
                        self.observe_result(
                            &operation_id,
                            Some(&Error::FenceRejected(fence.reason)),
                        )?;
                        return Err(Error::FenceRejected(fence.reason));
                    }
                }
            }
        }
        let owners = response.get("owner").and_then(Value::as_array);
        if owners.is_some_and(Vec::is_empty) && rows.is_some_and(Vec::is_empty) {
            self.store
                .inner
                .uncertain_receipts
                .lock()
                .map_err(|_| Error::Storage)?
                .remove(&operation_id);
            self.observe_result(&operation_id, Some(&Error::Contention))?;
            return Err(Error::Contention);
        }
        let valid = acknowledged_receipt(
            &response,
            &receipt,
            &operation_id,
            &partition,
            &mutation_guard,
        );
        if !valid {
            self.store
                .inner
                .cleanup_only
                .store(true, std::sync::atomic::Ordering::Release);
            self.observe_result(
                &operation_id,
                Some(&Error::OutcomeUnknown {
                    operation_id: operation_id.clone(),
                }),
            )?;
            return Err(Error::OutcomeUnknown { operation_id });
        }
        self.store
            .inner
            .uncertain_receipts
            .lock()
            .map_err(|_| Error::Storage)?
            .remove(&operation_id);
        *pending = None;
        self.observe_result(&operation_id, None)?;
        // Test-only acknowledgement evidence follows the existing canonical
        // receipt validation, never merely an inner transport Ok response.
        #[cfg(feature = "test-controls")]
        if let Ok(frames) = self.frames.lock() {
            self.store.inner.transport.fixture_after_ack(
                &operation_id,
                frames
                    .iter()
                    .filter_map(|(identity, frame)| frame.observation(identity).ok())
                    .collect(),
            );
        }
        self.finished = true;
        Ok(operation_id)
    }
}

#[cfg(all(test, feature = "embedded", feature = "service"))]
mod structural_response_controls {
    use super::*;
    use crate::{StoreInner, codec::Partition, transport::Transport};
    use engram_domain::{MemoryRecord, Scope};
    use engram_memory::MemoryRepository;
    use futures::executor::block_on;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    /// Declared corruption control: delegate to the actual engine before altering its successful response.
    struct CorruptCommittedResponse {
        actual: Arc<dyn Transport>,
        armed: AtomicBool,
        writes: AtomicUsize,
        corruption: &'static str,
    }
    impl Transport for CorruptCommittedResponse {
        fn execute(&self, operation: &Operation) -> Result<Value> {
            let response = self.actual.execute(operation)?;
            if operation.mutates {
                self.writes.fetch_add(1, Ordering::SeqCst);
                if self.armed.swap(false, Ordering::SeqCst) {
                    return Ok(match self.corruption {
                        "missing" => json!({}),
                        "invalid_type" => json!({"receipt":false}),
                        "empty_owned" => {
                            let mut damaged = response;
                            damaged["receipt"] = json!([]);
                            damaged
                        }
                        _ => {
                            json!({"receipt":[{"public_id":operation.id,"partition":"substituted","payload":"{}"}]})
                        }
                    });
                }
            }
            Ok(response)
        }
        fn close(&self) -> Result<()> {
            self.actual.close()
        }
        fn reconcile_read(&self, operation: &Operation) -> Result<Value> {
            self.actual.reconcile_read(operation)
        }
        fn confirm_operation(&self, id: &str) -> Result<()> {
            self.actual.confirm_operation(id)
        }
        fn register_cleanup(&self, lease: crate::ownership::ScopeLease) -> Result<()> {
            self.actual.register_cleanup(lease)
        }
    }
    fn open(service: bool, root: &std::path::Path, partition: &Partition) -> HelixStore {
        if service {
            HelixStore::open_service(
                crate::config::ServiceConfig {
                    endpoint: std::env::var("HELIX_TEST_ENDPOINT")
                        .expect("actual pinned service fixture required"),
                    credential_env: std::env::var("HELIX_TEST_CREDENTIAL_ENV")
                        .expect("explicit fixture credential reference required"),
                    vectors_enabled: true,
                    read_only: false,
                    timeout: std::time::Duration::from_secs(120),
                },
                partition.clone(),
            )
            .unwrap()
        } else {
            HelixStore::open_embedded(
                crate::config::EmbeddedConfig {
                    trusted_root: root.into(),
                    path: root.join("store"),
                    read_only: false,
                    queue_capacity: 8,
                },
                partition.clone(),
            )
            .unwrap()
        }
    }
    fn exercise(service: bool) {
        block_on(async {
            for corruption in ["missing", "invalid_type", "empty_owned", "wrong_identity"] {
                let temporary = tempfile::tempdir().unwrap();
                let partition = Partition {
                    logical_store: format!("structural-unknown-{}", uuid::Uuid::new_v4()),
                    scope: Scope {
                        tenant: "tenant".into(),
                        subject: None,
                        workspace: Some("workspace".into()),
                        session: None,
                        environment: None,
                    },
                    embedding_space: None,
                };
                let actual = open(service, temporary.path(), &partition);
                let control = Arc::new(CorruptCommittedResponse {
                    actual: actual.inner.transport.clone(),
                    armed: AtomicBool::new(true),
                    writes: AtomicUsize::new(0),
                    corruption,
                });
                let store = HelixStore {
                    inner: Arc::new(StoreInner {
                        transport: control.clone(),
                        mode: actual.inner.mode,
                        partition: partition.clone(),
                        lease: actual.inner.lease.clone(),
                        reservation: AtomicBool::new(false),
                        cleanup_only: AtomicBool::new(false),
                        uncertain_receipts: Mutex::new(BTreeMap::new()),
                    }),
                    session: None,
                    reentrant: false,
                    step_id: None,
                };
                let sibling = store.clone();
                let record:MemoryRecord=serde_json::from_value(json!({"id":"committed-once","kind":"fact","content":{"text":"committed before corrupted success"},"scope":partition.scope,"status":"active","createdAt":"2026-11-17T00:00:00Z","provenance":{"source":"response-control","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"workspace","retention":"durable"}})).unwrap();
                let error = store.put_memory(record.clone()).await.unwrap_err();
                let id = store
                    .inner
                    .uncertain_receipts
                    .lock()
                    .unwrap()
                    .keys()
                    .next()
                    .cloned()
                    .expect("exact operation retained");
                assert!(error.to_string().contains(&id));
                assert!(error.to_string().contains("outcome unknown"));
                let expected = store
                    .inner
                    .uncertain_receipts
                    .lock()
                    .unwrap()
                    .get(&id)
                    .unwrap()
                    .expected
                    .clone();
                assert_eq!(control.writes.load(Ordering::SeqCst), 1);
                assert!(
                    sibling
                        .get_memory(&record.id, &partition.scope)
                        .await
                        .is_err()
                );
                assert!(sibling.put_memory(record.clone()).await.is_err());
                assert!(sibling.count_memory_partitions().is_err());
                assert!(sibling.partition_memories(&partition.scope, 1, 0).is_err());
                assert!(sibling.get_sentinel("forbidden").is_err());
                assert!(sibling.reconcile_sentinel("forbidden", &id).is_err());
                assert!(sibling.scope_session().is_err());
                let receipt = sibling.reconcile_operation(&id).unwrap();
                assert_eq!(receipt, expected);
                assert!(sibling.put_memory(record.clone()).await.is_err());
                assert!(sibling.count_memory_partitions().is_err());
                assert_eq!(
                    control.writes.load(Ordering::SeqCst),
                    1,
                    "no mutation replay"
                );
                println!(
                    "TASK004 structural-success service={service} corruption={corruption} writes=1 exact-receipt={receipt}"
                );
                store.close().unwrap();
                drop(sibling);
                drop(store);
                drop(control);
                drop(actual);
                let reopened = open(service, temporary.path(), &partition);
                assert_eq!(
                    reopened
                        .get_memory(&record.id, &partition.scope)
                        .await
                        .unwrap(),
                    Some(record)
                );
                reopened.close().unwrap();
            }
        });
    }
    #[test]
    fn actual_embedded_malformed_committed_success_is_unknown_and_reconciles() {
        exercise(false)
    }
    #[test]
    fn actual_service_malformed_committed_success_is_unknown_and_reconciles() {
        exercise(true)
    }
    fn lifecycle_race(service: bool) {
        block_on(async {
            use engram_domain::{UnresolvedReference, UnresolvedReferenceStatus};
            use engram_knowledge::KnowledgeGraphRepository;
            let temporary = tempfile::tempdir().unwrap();
            let partition = Partition {
                logical_store: format!("step-race-{}", uuid::Uuid::new_v4()),
                scope: Scope {
                    tenant: "tenant".into(),
                    subject: None,
                    workspace: Some("workspace".into()),
                    session: None,
                    environment: None,
                },
                embedding_space: None,
            };
            let store = open(service, temporary.path(), &partition);
            let session = store.scope_session().unwrap();
            let scoped = session.store();
            let old = scoped.begin_step().unwrap();
            let entered = Arc::new(std::sync::Barrier::new(2));
            let release = Arc::new(std::sync::Barrier::new(2));
            *scoped.session.as_ref().unwrap().stage_pause.lock().unwrap() =
                Some(crate::session::StagePause {
                    entered: entered.clone(),
                    release: release.clone(),
                });
            let make = |id: String| {
                serde_json::from_value::<UnresolvedReference>(json!({"id":id,"scope":partition.scope,"fromEntityId":"left","referenceName":"unknown","status":"pending","path":"file","createdAt":"2026-11-17T00:00:00Z"})).unwrap()
            };
            let rows = (0..500)
                .map(|i| make(format!("abandoned-{i:04}")))
                .collect();
            let worker = scoped.clone();
            let thread = std::thread::spawn(move || block_on(worker.put_unresolved_refs(rows)));
            entered.wait();
            drop(old);
            assert!(
                matches!(scoped.begin_step(), Err(Error::Contention)),
                "new lifecycle cannot replace an active call"
            );
            release.wait();
            assert!(
                thread.join().unwrap().is_err(),
                "abandoned call must fail explicitly"
            );
            let replacement = scoped.begin_step().unwrap();
            scoped
                .put_unresolved_refs(vec![make("new-step".into())])
                .await
                .unwrap();
            replacement.commit().unwrap();
            drop(scoped);
            drop(session);
            store.close().unwrap();
            drop(store);
            let reopened = open(service, temporary.path(), &partition);
            let durable = reopened
                .list_unresolved_refs(&partition.scope, UnresolvedReferenceStatus::Pending)
                .await
                .unwrap();
            assert_eq!(durable.len(), 1);
            assert_eq!(durable[0].id.to_string(), "new-step");
            assert_eq!(
                durable
                    .iter()
                    .filter(|r| r.id.to_string().starts_with("abandoned-"))
                    .count(),
                0
            );
            reopened.close().unwrap();
            println!(
                "TASK004 synchronized public-port lifecycle service={service}: 500 abandoned rows, zero durable leaked rows, replacement row retained"
            );
        });
    }
    #[test]
    fn actual_embedded_abandoned_active_call_cannot_contaminate_replacement() {
        lifecycle_race(false)
    }
    #[test]
    fn actual_service_abandoned_active_call_cannot_contaminate_replacement() {
        lifecycle_race(true)
    }
}

#[cfg(all(test, feature = "embedded"))]
mod native_error_diagnostic {
    use super::*;
    use engram_domain::{
        Actor, ActorKind, EntityKind, EvidenceRef, EvidenceTargetType, KnowledgeEntity, Provenance,
        Scope,
    };
    use engram_knowledge::KnowledgeRepository;
    use futures::executor::block_on;
    use serde_json::json;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_owned()
        } else {
            "non-string panic payload".into()
        }
    }
    fn report(root: &std::path::Path, value: &Value) {
        match serde_json::to_vec_pretty(value) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(root.join("diagnostic.json"), bytes) {
                    println!("NATIVE_DIAGNOSTIC_REPORT_WRITE_ERROR {error}");
                }
            }
            Err(error) => println!("NATIVE_DIAGNOSTIC_REPORT_ENCODING_ERROR {error}"),
        }
        println!("NATIVE_DIAGNOSTIC {value}");
    }
    #[test]
    #[ignore = "one authorized disposable native diagnostic child; retained data required"]
    fn fresh_public_entity_native_phases() {
        let root = std::path::PathBuf::from(
            std::env::var("HELIX_NATIVE_DIAGNOSTIC_ROOT")
                .expect("explicit retained diagnostic root"),
        );
        assert!(!root.exists(), "fresh retained directory required");
        let scope = Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("workspace".into()),
            session: None,
            environment: Some("test".into()),
        };
        let partition = crate::codec::Partition {
            logical_store: format!("native-diagnostic-{}", uuid::Uuid::new_v4()),
            scope: scope.clone(),
            embedding_space: None,
        };
        let timestamp: chrono::DateTime<chrono::Utc> = "2026-11-17T00:00:00Z".parse().unwrap();
        let id = format!("diagnostic-{}", uuid::Uuid::new_v4());
        let entity = KnowledgeEntity {
            id: id.clone().into(),
            graph_id: Some("graph".into()),
            kind: EntityKind::Concept,
            name: id,
            aliases: vec![],
            scope: scope.clone(),
            source_refs: vec![EvidenceRef {
                target_type: EvidenceTargetType::Source,
                target_id: Some("other-source".into()),
                uri: None,
                quote: None,
                location: None,
            }],
            concept_refs: vec![],
            ontology_class_refs: vec![],
            provenance: Provenance {
                source: "fixture".into(),
                actor: Actor {
                    id: "actor".into(),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: timestamp,
                evidence: vec![],
                derivations: vec![],
                confidence: None,
                method: None,
            },
            created_at: timestamp,
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        };
        let discriminator = partition.discriminator().unwrap();
        assert_eq!(entity.scope, partition.scope);
        assert_eq!(
            serde_json::from_value::<KnowledgeEntity>(serde_json::to_value(&entity).unwrap())
                .unwrap(),
            entity
        );
        assert_eq!(
            serde_json::from_value::<Scope>(serde_json::to_value(&scope).unwrap()).unwrap(),
            scope
        );
        std::fs::create_dir_all(&root).unwrap();
        let mut value = json!({"kind":"investigation_only_not_provider_verification","partition_discriminator":discriminator,"logical_store":partition.logical_store,"scope":scope,"fixture_validated_before_engine":true,"original_operation_replayed":false,"retained_data":true});
        let opened = catch_unwind(AssertUnwindSafe(|| {
            HelixStore::open_embedded(
                crate::config::EmbeddedConfig {
                    trusted_root: root.clone(),
                    path: root.join("store"),
                    read_only: false,
                    queue_capacity: 8,
                },
                partition,
            )
        }));
        let store = match opened {
            Ok(Ok(store)) => std::mem::ManuallyDrop::new(store),
            Ok(Err(error)) => {
                value["open_error"] = json!(format!("{error:?}"));
                value["native_close"] = json!("NOT_ATTEMPTED_NO_HANDLE");
                report(&root, &value);
                panic!("positive native fixture open failed; custody reported");
            }
            Err(panic) => {
                value["open_panic"] = json!(panic_text(panic));
                value["native_close"] = json!("NOT_ATTEMPTED_NO_HANDLE");
                report(&root, &value);
                panic!("positive native fixture open failed; custody reported");
            }
        };
        let written = catch_unwind(AssertUnwindSafe(|| {
            crate::embedded::begin_native_diagnostic();
            block_on(store.put_entity(entity))
        }));
        value["public_result"] = match written {
            Ok(Ok(_)) => json!("success"),
            Ok(Err(error)) => json!({"error":format!("{error:?}")}),
            Err(panic) => json!({"panic":panic_text(panic)}),
        };
        value["native_phases"] =
            match catch_unwind(AssertUnwindSafe(crate::embedded::finish_native_diagnostic)) {
                Ok(phases) => json!(phases),
                Err(panic) => json!({"capture_panic":panic_text(panic)}),
            };
        // Independent close attempt is mandatory even when the single public write panics.
        value["native_close"] = match catch_unwind(AssertUnwindSafe(|| store.close())) {
            Ok(result) => json!(format!("{result:?}")),
            Err(panic) => json!({"close_panic":panic_text(panic)}),
        };
        value["store_drop"] = json!("intentionally_suppressed_retained_data");
        report(&root, &value);
        assert_eq!(
            value["public_result"],
            json!("success"),
            "fresh ordinary write must succeed; custody reported"
        );
        assert_eq!(
            value["native_close"],
            json!("Ok(())"),
            "native close must succeed; custody reported"
        );
    }
}
