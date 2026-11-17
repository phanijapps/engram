#![allow(dead_code)]
use engram_domain::*;
#[cfg(feature = "embedded")]
use engram_store_helixdb::config::EmbeddedConfig;
#[cfg(feature = "service")]
use engram_store_helixdb::config::ServiceConfig;
use engram_store_helixdb::{HelixStore, codec::Partition};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum Mode {
    #[cfg(feature = "embedded")]
    Embedded,
    #[cfg(feature = "service")]
    Service,
}
pub struct Fixture {
    pub store: HelixStore,
    pub partition: Partition,
    pub mode: Mode,
    temporary: Option<tempfile::TempDir>,
    root: std::path::PathBuf,
    identity: String,
    handle_generation: u64,
    opening_generation: u64,
    handle_partition: Partition,
    unconfirmed_open: bool,
}
impl Fixture {
    pub fn new(mode: Mode) -> Self {
        Self::with_space(mode, None)
    }
    pub fn with_space(mode: Mode, embedding_space: Option<EmbeddingSpace>) -> Self {
        let temporary = tempfile::Builder::new()
            .prefix("helix-owned-fixture-")
            .tempdir()
            .unwrap();
        let root = temporary.path().to_owned();
        let identity = uuid::Uuid::new_v4().to_string();
        let partition = Partition {
            logical_store: format!("task004-{}", uuid::Uuid::new_v4()),
            scope: scope(),
            embedding_space,
        };
        let store = match Self::open(mode, &root, &partition, false, &identity, "initial-open", 1) {
            Ok(store) => store,
            Err(error) => {
                let retained = temporary.keep();
                eprintln!(
                    "HELIX_FIXTURE {}",
                    json!({"fixture":identity,"phase":"initial-open-failed","root":retained,"error":error,"native_close":"NOT_ATTEMPTED_NO_RETURNED_HANDLE","data_retained":true})
                );
                panic!("fixture initial open failed; identified data retained: {error}");
            }
        };
        Self {
            store,
            handle_partition: partition.clone(),
            partition,
            mode,
            temporary: Some(temporary),
            root,
            identity,
            handle_generation: 1,
            opening_generation: 1,
            unconfirmed_open: false,
        }
    }
    fn open(
        mode: Mode,
        root: &std::path::Path,
        partition: &Partition,
        read_only: bool,
        identity: &str,
        phase: &str,
        generation: u64,
    ) -> Result<HelixStore, String> {
        eprintln!(
            "HELIX_FIXTURE {}",
            json!({"fixture":identity,"phase":phase,"event":"before-acquisition","generation":generation,"mode":format!("{mode:?}"),"root":root,"database_path":root.join("store"),"logical_store":partition.logical_store,"scope":partition.scope,"partition":partition.discriminator().map_err(|e|format!("{e:?}")),"backtrace":std::backtrace::Backtrace::force_capture().to_string()})
        );
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match mode {
            #[cfg(feature = "embedded")]
            Mode::Embedded => HelixStore::open_embedded(
                EmbeddedConfig {
                    trusted_root: root.into(),
                    path: root.join("store"),
                    read_only,
                    queue_capacity: 8,
                },
                partition.clone(),
            ),
            #[cfg(feature = "service")]
            Mode::Service => HelixStore::open_service(
                ServiceConfig {
                    endpoint: std::env::var("HELIX_TEST_ENDPOINT")
                        .expect("actual pinned service endpoint required"),
                    credential_env: std::env::var("HELIX_TEST_CREDENTIAL_ENV")
                        .expect("actual explicit credential fixture required"),
                    vectors_enabled: true,
                    read_only,
                    timeout: std::time::Duration::from_secs(120),
                },
                partition.clone(),
            ),
        }));
        match opened {
            Ok(Ok(store)) => {
                eprintln!(
                    "HELIX_FIXTURE {}",
                    json!({"fixture":identity,"phase":phase,"event":"acquired","generation":generation,"root":root})
                );
                Ok(store)
            }
            Ok(Err(error)) => Err(format!("{error:?}")),
            Err(payload) => Err(format!("panic: {}", panic_message(payload))),
        }
    }
    fn retain(&mut self, reason: &str) {
        if let Some(temporary) = self.temporary.take() {
            let retained = temporary.keep();
            eprintln!(
                "HELIX_FIXTURE {}",
                json!({"fixture":self.identity,"phase":"retain","reason":reason,"root":retained,"handle_generation":self.handle_generation,"opening_generation":self.opening_generation,"unconfirmed_open":self.unconfirmed_open,"data_retained":true})
            );
        }
    }
    fn close_recorded(&self, phase: &str) -> Result<(), String> {
        let closed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.store.close()));
        let result = match closed {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("{error:?}")),
            Err(payload) => Err(format!("panic: {}", panic_message(payload))),
        };
        eprintln!(
            "HELIX_FIXTURE {}",
            json!({"fixture":self.identity,"phase":phase,"root":self.root,"handle_generation":self.handle_generation,"handle_logical_store":self.handle_partition.logical_store,"handle_scope":self.handle_partition.scope,"opening_generation":self.opening_generation,"unconfirmed_new_open":self.unconfirmed_open,"native_close_for_held_handle":result})
        );
        result
    }
    fn replace_open(&mut self, phase: &str) {
        if let Err(error) = self.close_recorded("before-reopen-held-handle") {
            self.retain("held-handle-close-failed");
            panic!("fixture close before {phase} failed; data retained: {error}");
        }
        self.opening_generation += 1;
        self.unconfirmed_open = true;
        match Self::open(
            self.mode,
            &self.root,
            &self.partition,
            false,
            &self.identity,
            phase,
            self.opening_generation,
        ) {
            Ok(store) => {
                self.store = store;
                self.handle_generation = self.opening_generation;
                self.handle_partition = self.partition.clone();
                self.unconfirmed_open = false;
            }
            Err(error) => {
                self.retain("new-open-unconfirmed-old-handle-confirmed-closed");
                eprintln!(
                    "HELIX_FIXTURE {}",
                    json!({"fixture":self.identity,"phase":phase,"root":self.root,"error":error,"old_handle_close":"CONFIRMED","new_open_close":"NOT_ATTEMPTED_NO_RETURNED_HANDLE"})
                );
                panic!("fixture {phase} failed; identified data retained: {error}");
            }
        }
    }
    pub fn reopen(&mut self) {
        self.replace_open("reopen");
    }
    pub fn switch_partition(&mut self, scope: Scope, logical_store: String) {
        // The held handle retains its original partition identity until replacement succeeds.
        self.partition.scope = scope;
        self.partition.logical_store = logical_store;
        self.replace_open("partition-open");
    }
    pub fn reader(&self) -> HelixStore {
        match Self::open(
            self.mode,
            &self.root,
            &self.partition,
            true,
            &self.identity,
            "reader-open",
            self.handle_generation,
        ) {
            Ok(store) => store,
            Err(error) => panic!(
                "fixture reader open failed; owner unwind will retain identified root {}: {error}",
                self.root.display()
            ),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let unwinding = std::thread::panicking();
        if unwinding || self.unconfirmed_open {
            self.retain("test-unwind-or-unconfirmed-opening");
        }
        if let Err(error) = self.close_recorded("fixture-drop-held-handle") {
            self.retain("native-close-unconfirmed");
            if !unwinding {
                panic!("fixture owned close failed; identified data retained: {error}");
            }
        }
    }
}
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else {
        "non-string panic payload".into()
    }
}
pub fn scope() -> Scope {
    Scope {
        tenant: "tenant".into(),
        subject: None,
        workspace: Some("workspace".into()),
        session: None,
        environment: Some("test".into()),
    }
}
pub fn base(id: &str) -> Value {
    json!({"id":id,"scope":scope(),"name":id,"kind":"unknown","createdAt":"2026-11-17T00:00:00Z","provenance":{"source":"fixture","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}})
}
pub fn make<T: DeserializeOwned>(id: &str, patch: Value) -> T {
    let mut v = base(id);
    for (k, val) in patch.as_object().unwrap() {
        v[k] = val.clone();
    }
    serde_json::from_value(v).unwrap()
}
pub fn entity(id: &str, sources: &[&str]) -> KnowledgeEntity {
    make(
        id,
        json!({"kind":"concept","graphId":"graph","sourceRefs":sources.iter().map(|s|json!({"targetType":"source","targetId":s})).collect::<Vec<_>>()}),
    )
}
pub fn relation(id: &str, sources: &[&str]) -> KnowledgeRelationship {
    make(
        id,
        json!({"graphId":"graph","subject":{"id":"left"},"predicate":"related","object":{"id":"right"},"evidence":sources.iter().map(|s|json!({"targetType":"source","targetId":s})).collect::<Vec<_>>()}),
    )
}
pub fn memory(id: &str) -> MemoryRecord {
    make(
        id,
        json!({"kind":"fact","content":{"text":"governed fixture"},"status":"active"}),
    )
}
pub fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) {
    assert_eq!(
        &serde_json::from_value::<T>(serde_json::to_value(v).unwrap()).unwrap(),
        v
    );
}

pub fn code_generation(id: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.into(),
        binding_version: 1,
        commit_oid: "1".repeat(40),
        snapshot_digest: "2".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    }
}
pub fn code_attempt(id: &str, epoch: u64, active: Option<&str>) -> CodeAttempt {
    CodeAttempt {
        id: id.into(),
        idempotency_key: id.into(),
        binding_version: 1,
        expected_epoch: epoch,
        expected_active_generation: active.map(str::to_owned),
        fencing_token: 0,
        state: CodeAttemptState::Created,
        receipt_kinds: vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge],
    }
}
pub fn code_lineage(repo: &str, generation: &str) -> CodeLineage {
    CodeLineage {
        repo_id: RepoId(repo.into()),
        generation_id: generation.into(),
        source_path: "src/lib.rs".into(),
        contributors: vec!["source-role".into()],
        span: Some(CanonicalSpan {
            start_line: 1,
            end_line: 2,
        }),
    }
}
pub async fn ready_code(
    store: &HelixStore,
    repo: &RepoId,
    generation: &str,
    attempt: &str,
    epoch: u64,
    active: Option<&str>,
) -> CodeAttempt {
    use engram_knowledge::CodeBindingRepository;
    let a = store
        .begin_attempt(
            &scope(),
            repo,
            code_generation(generation),
            code_attempt(attempt, epoch, active),
        )
        .await
        .unwrap();
    for lane in ["source", "knowledge"] {
        store
            .record_receipt(engram_knowledge::StageReceipt {
                attempt_id: a.id.clone(),
                generation_id: generation.into(),
                lane: lane.into(),
                digest: format!("digest-{lane}"),
            })
            .await
            .unwrap();
    }
    store
        .mark_ready(&scope(), repo, &a.id, a.fencing_token)
        .await
        .unwrap()
}
