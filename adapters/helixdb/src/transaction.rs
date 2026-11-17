//! One storage transaction per acknowledged step; earlier steps remain durable.
use crate::{
    HelixStore,
    error::{Error, Result},
    records::{self, EDGE, ENDPOINT, RELATIONSHIP},
    transport::Operation,
};
use helix_db::dsl::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
pub(crate) struct StepState {
    pub(crate) id: String,
    pub(crate) mutations: BTreeMap<(String, String), Option<Value>>,
}
/// Explicit step boundary. Drop abandons staged work only.
pub struct StorageStep {
    pub(crate) id: String,
    store: HelixStore,
    finished: bool,
}
impl Drop for StorageStep {
    fn drop(&mut self) {
        if !self.finished {
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
        *pending = Some(StepState {
            id: id.clone(),
            mutations: BTreeMap::new(),
        });
        let mut store = self.clone();
        store.step_id = Some(id.clone());
        Ok(StorageStep {
            id,
            store,
            finished: false,
        })
    }
}
impl StorageStep {
    pub fn commit(mut self) -> Result<String> {
        self.store.admit_logical_work()?;
        let _call = self.store.enter_call()?;
        let state = self.store.session.as_ref().ok_or(Error::Storage)?;
        let mut pending = state.pending.lock().map_err(|_| Error::Storage)?;
        let step = pending
            .as_mut()
            .filter(|step| step.id == self.id)
            .ok_or(Error::Contention)?;
        let explicit_mutation_count = step.mutations.len();
        crate::changes::augment(&self.store, &mut step.mutations)?;
        let operation_id = uuid::Uuid::new_v4().to_string();
        let lease = self.store.inner.lease.as_ref().ok_or(Error::ReadOnly)?;
        let partition = self.store.inner.partition.discriminator()?;
        let mut batch = lease.guard();
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
                BatchCondition::VarMinSize("owner".into(), 1),
                g().n(NodeRef::var(&existing)).drop(),
            );
            if family == RELATIONSHIP {
                let old = format!("oldedge{i}");
                batch = batch.var_as_if(
                    &old,
                    BatchCondition::VarMinSize("owner".into(), 1),
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
                    BatchCondition::VarMinSize("owner".into(), 1),
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
                    BatchCondition::VarMinSize("owner".into(), 1),
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
        let mut receipt = crate::receipts::receipt(&self.store, &operation_id, &step.mutations)?;
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
                BatchCondition::VarMinSize("owner".into(), 1),
                g().add_n(
                    records::RECEIPT,
                    records::properties(&self.store, records::RECEIPT, &operation_id, &receipt)?,
                )
                .value_map(Some(vec!["public_id", "partition", "payload"])),
            )
            .returning(["owner", "receipt"]);
        let operation = Operation::with_id(QueryRequest::write(batch), operation_id.clone())?;
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
                return Err(error);
            }
        };
        let rows = response.get("receipt").and_then(Value::as_array);
        let owners = response.get("owner").and_then(Value::as_array);
        if owners.is_some_and(Vec::is_empty) && rows.is_some_and(Vec::is_empty) {
            self.store
                .inner
                .uncertain_receipts
                .lock()
                .map_err(|_| Error::Storage)?
                .remove(&operation_id);
            return Err(Error::Contention);
        }
        let valid = rows.is_some_and(|rows| {
            owners.is_some_and(|owners| owners.len() == 1)
                && rows.len() == 1
                && rows[0].get("public_id").and_then(Value::as_str) == Some(operation_id.as_str())
                && rows[0].get("partition").and_then(Value::as_str) == Some(partition.as_str())
                && rows[0]
                    .get("payload")
                    .and_then(Value::as_str)
                    .and_then(|v| serde_json::from_str::<Value>(v).ok())
                    .as_ref()
                    == Some(&receipt)
        });
        if !valid {
            self.store
                .inner
                .cleanup_only
                .store(true, std::sync::atomic::Ordering::Release);
            return Err(Error::OutcomeUnknown { operation_id });
        }
        self.store
            .inner
            .uncertain_receipts
            .lock()
            .map_err(|_| Error::Storage)?
            .remove(&operation_id);
        *pending = None;
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
