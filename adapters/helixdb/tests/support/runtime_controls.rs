//! Declared negative-control workers only. Real storage positives live in primitive integration tests.
use super::*;
use helix_db::{
    QueryRequest,
    dsl::prelude::{g, write_batch},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct FaultWorker {
    fail_close: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    mode: String,
    drops: Arc<AtomicUsize>,
    cleanup_calls: Arc<AtomicUsize>,
}
impl Engine for FaultWorker {
    fn startup_status(&self) -> Result<()> {
        if self.mode == "opaque-engine" {
            Err(Error::CleanupUnconfirmed)
        } else {
            Ok(())
        }
    }
    fn execute(&mut self, operation: &Operation) -> Result<Value> {
        if operation.mutates {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
        if self.mode == "late-query" {
            if operation.mutates {
                thread::sleep(Duration::from_millis(80));
            }
            return Ok(serde_json::json!({"confirmed": true}));
        }
        if self.mode == "panic" {
            panic!("declared worker-panic control");
        }
        Err(operation.unknown())
    }
    fn cleanup(&mut self) -> Result<()> {
        self.cleanup_calls.fetch_add(1, Ordering::SeqCst);
        if self.mode == "late-close" {
            thread::sleep(Duration::from_millis(80));
        }
        if self.fail_close.load(Ordering::SeqCst) {
            Err(Error::CleanupUnconfirmed)
        } else {
            Ok(())
        }
    }
    fn register_cleanup(&mut self, _lease: crate::ownership::ScopeLease) -> Result<()> {
        Ok(())
    }
}

impl Drop for FaultWorker {
    fn drop(&mut self) {
        if self.mode == "late-join" {
            thread::sleep(Duration::from_millis(80));
        }
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn cleanup_failure_panic_channel_and_unknown_custody_controls() {
    let mode = match std::env::var("ENGRAM_HELIX_RUNTIME_CONTROL") {
        Ok(mode) => mode,
        Err(_) => {
            for mode in [
                "close",
                "panic",
                "channel",
                "opaque-start",
                "opaque-engine",
                "unknown",
                "late-start",
                "late-query",
                "late-close",
                "late-join",
            ] {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "runtime::controls::cleanup_failure_panic_channel_and_unknown_custody_controls", "--nocapture"])
                    .env("ENGRAM_HELIX_RUNTIME_CONTROL", mode).status().unwrap();
                assert!(status.success(), "runtime control failed: {mode}");
            }
            return;
        }
    };
    let temporary = tempfile::tempdir().unwrap();
    let lock_path = temporary.path().join("writer.lock");
    let lock = Some(WriterLock::acquire(&lock_path).unwrap());
    let flag = Arc::new(AtomicBool::new(mode == "close"));
    let calls = Arc::new(AtomicUsize::new(0));
    let admission = Admission::acquire().unwrap();
    if mode.starts_with("late-") || mode == "opaque-engine" {
        retained_completion_control(&mode, admission, lock, &lock_path);
        return;
    }
    if mode == "opaque-start" {
        assert!(matches!(
            admission.start(1, lock, Duration::from_millis(100), || Err(
                Error::CleanupUnconfirmed
            )),
            Err(Error::CleanupUnconfirmed)
        ));
        assert!(WriterLock::acquire(&lock_path).is_err());
        assert!(Admission::acquire().is_err());
        assert!(retry_retained_cleanup().is_err());
        return;
    }
    let owner = if mode == "channel" {
        // A disconnected owned channel is never proof of cleanup.
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let join = thread::spawn(|| {});
        let mut admission = admission;
        admission.transferred = true;
        Arc::new(RuntimeOwner {
            lease: Mutex::new(Some(Lease {
                sender,
                join: Some(join),
                _lock: lock,
                poisoned: false,
                cleanup_only: false,
                unknown_operation: None,
                blocked_admission: false,
                timeout: Duration::from_millis(100),
                startup_pending: None,
                pending: None,
                registration_pending: None,
                close_pending: None,
                engine_closed: false,
                retained_response: None,
            })),
        })
    } else {
        let worker = FaultWorker {
            fail_close: flag.clone(),
            calls: calls.clone(),
            mode: mode.clone(),
            drops: Arc::new(AtomicUsize::new(0)),
            cleanup_calls: Arc::new(AtomicUsize::new(0)),
        };
        admission
            .start(1, lock, Duration::from_millis(100), move || {
                Ok(Box::new(worker))
            })
            .unwrap()
    };
    let operation = Operation::new(QueryRequest::write(write_batch().var_as(
        "write",
        g().add_n("DeclaredNegativeControl", vec![("key", "value")]),
    )))
    .unwrap();
    assert!(operation.mutates);
    if mode == "close" {
        assert!(owner.close().is_err());
    } else {
        assert!(owner.execute(&operation).is_err());
    }
    assert!(
        Admission::acquire().is_err(),
        "unresolved live owner must block admission before final Drop"
    );
    assert!(
        retry_retained_cleanup().is_err(),
        "empty quarantine cannot imply failed live owner cleanup succeeded"
    );
    if mode == "unknown" {
        let wrong = Operation::with_id(
            QueryRequest::write(write_batch()),
            "wrong-receipt-id".into(),
        )
        .unwrap();
        assert!(!wrong.mutates);
        let error = owner.reconcile_read(&wrong).unwrap_err();
        assert_eq!(
            error.outcome_unknown_operation(),
            Some(operation.id.as_str())
        );
        let read =
            Operation::with_id(QueryRequest::write(write_batch()), operation.id.clone()).unwrap();
        let error = owner.reconcile_read(&read).unwrap_err();
        assert_eq!(
            error.outcome_unknown_operation(),
            Some(operation.id.as_str())
        );
        assert!(owner.close().is_err());
    }
    let before = calls.load(Ordering::SeqCst);
    assert!(owner.execute(&operation).is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "second logical write must not reach worker"
    );
    drop(owner);
    assert!(
        WriterLock::acquire(&lock_path).is_err(),
        "unconfirmed cleanup must retain writer ownership"
    );
    assert!(
        Admission::acquire().is_err(),
        "unresolved custody blocks admission"
    );
    if mode == "close" {
        flag.store(false, Ordering::SeqCst);
        retry_retained_cleanup().unwrap();
        assert!(WriterLock::acquire(&lock_path).is_ok());
        assert!(Admission::acquire().is_ok());
    } else {
        assert!(retry_retained_cleanup().is_err());
    }
    // Each failed-worker case runs in its own subprocess; process exit establishes final quiescence.
}

#[cfg(feature = "embedded")]
#[test]
fn successful_engine_acquisition_failed_marker_start_closes_and_releases() {
    use crate::{HelixStore, codec::Partition, config::EmbeddedConfig};
    let temporary = tempfile::tempdir().unwrap();
    let config = EmbeddedConfig {
        trusted_root: temporary.path().into(),
        path: temporary.path().join("store"),
        read_only: false,
        queue_capacity: 2,
    };
    let partition = Partition {
        logical_store: "failed-start".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
    };
    let store = HelixStore::open_embedded(config.clone(), partition.clone()).unwrap();
    store.close().unwrap();
    drop(store);
    let acquired =
        Arc::new(crate::embedded::Embedded::open(config.clone(), partition.clone()).unwrap());
    let mut marker = crate::codec::Marker::expected(&partition);
    marker.schema_version = 2;
    // Declared later-startup-failure control: the real engine is already acquired.
    use crate::transport::Transport;
    let request = QueryRequest::write(
        write_batch()
            .var_as(
                "marker",
                g().n_with_label_where(
                    crate::schema::MARKER_LABEL,
                    helix_db::SourcePredicate::eq(
                        "key",
                        partition
                            .record_key(crate::schema::MARKER_LABEL, "schema")
                            .unwrap(),
                    ),
                )
                .set_property("payload", serde_json::to_string(&marker).unwrap()),
            )
            .returning(["marker"]),
    );
    acquired.execute(&Operation::new(request).unwrap()).unwrap();
    assert!(matches!(
        HelixStore::finish_open(acquired, partition.clone(), false),
        Err(Error::IncompatibleSchema)
    ));
    assert!(WriterLock::acquire(&config.path.join("engram-writer.lock")).is_ok());
    let epoch = native_epoch(&config);
    assert!(matches!(
        HelixStore::open_embedded(config.clone(), partition),
        Err(Error::IncompatibleSchema)
    ));
    assert_eq!(
        native_epoch(&config),
        epoch,
        "newer marker must reject before native writer epoch advances"
    );
    assert!(WriterLock::acquire(&config.path.join("engram-writer.lock")).is_ok());
}

#[test]
fn cross_partition_response_is_rejected_before_payload_leaves_adapter() {
    struct WrongPartition;
    impl crate::transport::Transport for WrongPartition {
        fn execute(&self, _: &Operation) -> Result<Value> {
            Ok(
                serde_json::json!({"rows":[{"public_id":"id", "partition":"foreign", "payload":"secret"}]}),
            )
        }
        fn close(&self) -> Result<()> {
            Ok(())
        }
        fn register_cleanup(&self, _: crate::ownership::ScopeLease) -> Result<()> {
            Ok(())
        }
        fn reconcile_read(&self, _: &Operation) -> Result<Value> {
            Err(Error::Storage)
        }
        fn confirm_operation(&self, _: &str) -> Result<()> {
            Err(Error::Storage)
        }
    }
    let partition = crate::codec::Partition {
        logical_store: "local".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
    };
    assert!(matches!(
        crate::schema::read_payload(&WrongPartition, &partition, "label", "id"),
        Err(Error::IncompatibleSchema)
    ));
}

#[test]
fn startup_unknown_cause_survives_unconfirmed_cleanup() {
    struct UnknownInitialization;
    impl crate::transport::Transport for UnknownInitialization {
        fn execute(&self, operation: &Operation) -> Result<Value> {
            if operation.write {
                Err(Error::OutcomeUnknown {
                    operation_id: "retained-initialization-id".into(),
                })
            } else {
                Ok(serde_json::json!({"rows": []}))
            }
        }
        fn close(&self) -> Result<()> {
            Err(Error::CleanupUnconfirmed)
        }
        fn register_cleanup(&self, _: crate::ownership::ScopeLease) -> Result<()> {
            Ok(())
        }
        fn reconcile_read(&self, _: &Operation) -> Result<Value> {
            Err(Error::Storage)
        }
        fn confirm_operation(&self, _: &str) -> Result<()> {
            Err(Error::Storage)
        }
    }
    let partition = crate::codec::Partition {
        logical_store: "startup-control".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
    };
    let error =
        match crate::HelixStore::finish_open(Arc::new(UnknownInitialization), partition, false) {
            Err(error) => error,
            Ok(_) => panic!("uncertain initialization cannot produce an open store"),
        };
    assert!(matches!(error, Error::CleanupAfterFailure { .. }));
    assert_eq!(
        error.outcome_unknown_operation(),
        Some("retained-initialization-id")
    );
    assert!(error.to_string().contains("cleanup unconfirmed"));
}

fn retained_completion_control(
    mode: &str,
    admission: Admission,
    lock: Option<WriterLock>,
    path: &std::path::Path,
) {
    let drops = Arc::new(AtomicUsize::new(0));
    let cleanups = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let opens = Arc::new(AtomicUsize::new(0));
    let opening = opens.clone();
    let opening_mode = mode.to_owned();
    let worker = FaultWorker {
        mode: mode.into(),
        fail_close: Arc::new(AtomicBool::new(mode == "opaque-engine")),
        calls: calls.clone(),
        drops: drops.clone(),
        cleanup_calls: cleanups.clone(),
    };
    let opened = admission.start(1, lock, Duration::from_millis(20), move || {
        opening.fetch_add(1, Ordering::SeqCst);
        if opening_mode == "late-start" {
            thread::sleep(Duration::from_millis(80));
        }
        Ok(Box::new(worker))
    });
    if mode == "opaque-engine" {
        assert!(opened.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert!(WriterLock::acquire(path).is_err());
        assert!(Admission::acquire().is_err());
        assert!(retry_retained_cleanup().is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        return;
    }
    if mode == "late-start" {
        assert!(opened.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert!(WriterLock::acquire(path).is_err());
        assert!(Admission::acquire().is_err());
        thread::sleep(Duration::from_millis(120));
        retry_retained_cleanup().unwrap();
    } else {
        let owner = opened.unwrap();
        if mode == "late-query" {
            let write = Operation::new(QueryRequest::write(
                write_batch().var_as("write", g().add_n("DeadlineControl", vec![("id", "one")])),
            ))
            .unwrap();
            let error = owner.execute(&write).unwrap_err();
            assert_eq!(error.outcome_unknown_operation(), Some(write.id.as_str()));
            assert!(owner.execute(&write).is_err());
            assert!(owner.close().is_err());
            assert!(WriterLock::acquire(path).is_err());
            assert!(Admission::acquire().is_err());
            thread::sleep(Duration::from_millis(120));
            let read =
                Operation::with_id(QueryRequest::write(write_batch()), write.id.clone()).unwrap();
            assert!(!read.mutates);
            assert_eq!(owner.reconcile_read(&read).unwrap()["confirmed"], true);
            owner.confirm_operation(&write.id).unwrap();
            assert!(owner.execute(&write).is_err());
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            owner.close().unwrap();
            drop(owner);
        } else {
            assert!(owner.close().is_err());
            assert!(WriterLock::acquire(path).is_err());
            assert!(Admission::acquire().is_err());
            drop(owner);
            assert_eq!(
                cleanups.load(Ordering::SeqCst),
                1,
                "late cleanup must not be resubmitted"
            );
            thread::sleep(Duration::from_millis(120));
            retry_retained_cleanup().unwrap();
        }
    }
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(cleanups.load(Ordering::SeqCst), 1);
    assert!(WriterLock::acquire(path).is_ok());
    assert!(Admission::acquire().is_ok());
}

#[cfg(feature = "service")]
#[test]
fn only_complete_pinned_conflict_and_backpressure_envelopes_are_definite_rejections() {
    use crate::service::classify_rejection;
    assert!(matches!(
        classify_rejection(
            409,
            br#"{"error":"transaction_conflict","msg":"transaction rejected"}"#
        ),
        Some(Error::Contention)
    ));
    assert!(matches!(
        classify_rejection(
            429,
            br#"{"error":"index_backpressure","msg":"saturated","retryable":true}"#
        ),
        Some(Error::Backpressure)
    ));
    for (status, body) in [
        (409, br#"{}"#.as_slice()),
        (
            409,
            br#"{"error":"transaction_conflict","msg":"rejected","retryable":false}"#,
        ),
        (409, br#"{"error":"unknown","msg":"rejected"}"#),
        (409, br#"{"error":"transaction_conflict""#),
        (
            500,
            br#"{"error":"transaction_conflict","msg":"uncertain"}"#,
        ),
        (429, br#"{"error":"index_backpressure","msg":"saturated"}"#),
    ] {
        assert!(classify_rejection(status, body).is_none());
    }
}

#[cfg(feature = "embedded")]
fn native_epoch(config: &crate::config::EmbeddedConfig) -> u64 {
    let path = config.path.clone();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let source = helix_db::HelixDbSource::Disk {
                    root: path,
                    database: "engram-v1".into(),
                };
                let reader = helix_db::HelixDB::open_reader_for_server(
                    source.clone(),
                    source.embedded_default_config(),
                )
                .await
                .unwrap();
                let epoch = reader.storage_writer_epoch().unwrap().get();
                reader.close().await.unwrap();
                epoch
            })
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn malformed_scope_cleanup_evidence_is_not_treated_as_released_or_foreign() {
    use std::cell::Cell;
    let lease = crate::ownership::ScopeLease {
        key: "expected-key".into(),
        partition: "expected-partition".into(),
        token: "our-owner".into(),
        epoch: 1,
    };
    for row in [
        serde_json::json!({"owner":"foreign"}),
        serde_json::json!({"key":"expected-key","partition":"foreign-partition","owner":"foreign","epoch":1}),
        serde_json::json!({"key":"foreign-key","partition":"expected-partition","owner":"foreign","epoch":1}),
        serde_json::json!({"key":"expected-key","partition":"expected-partition","owner":"foreign","epoch":null}),
    ] {
        let writes = Cell::new(0);
        assert!(
            crate::ownership::release(&lease, &mut |operation| {
                if operation.mutates {
                    writes.set(writes.get() + 1);
                }
                Ok(serde_json::json!({"lock":[row.clone()]}))
            })
            .is_err()
        );
        assert_eq!(writes.get(), 0);
    }
    let valid_foreign = serde_json::json!({"key":"expected-key","partition":"expected-partition","owner":"foreign","epoch":2});
    crate::ownership::release(&lease, &mut |operation| {
        assert!(!operation.mutates, "must never release another valid owner");
        Ok(serde_json::json!({"lock":[valid_foreign.clone()]}))
    })
    .unwrap();
}
