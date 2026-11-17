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
    temporary: tempfile::TempDir,
}
impl Fixture {
    pub fn new(mode: Mode) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let partition = Partition {
            logical_store: format!("task004-{}", uuid::Uuid::new_v4()),
            scope: scope(),
            embedding_space: None,
        };
        let store = Self::open(mode, &temporary, &partition, false);
        Self {
            store,
            partition,
            mode,
            temporary,
        }
    }
    fn open(
        mode: Mode,
        temporary: &tempfile::TempDir,
        partition: &Partition,
        read_only: bool,
    ) -> HelixStore {
        match mode {
            #[cfg(feature = "embedded")]
            Mode::Embedded => HelixStore::open_embedded(
                EmbeddedConfig {
                    trusted_root: temporary.path().into(),
                    path: temporary.path().join("store"),
                    read_only,
                    queue_capacity: 8,
                },
                partition.clone(),
            )
            .unwrap(),
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
            )
            .unwrap(),
        }
    }
    pub fn reopen(&mut self) {
        self.store.close().unwrap();
        self.store = Self::open(self.mode, &self.temporary, &self.partition, false);
    }
    pub fn switch_partition(&mut self, scope: Scope, logical_store: String) {
        self.store.close().unwrap();
        self.partition.scope = scope;
        self.partition.logical_store = logical_store;
        self.store = Self::open(self.mode, &self.temporary, &self.partition, false);
    }
    pub fn reader(&self) -> HelixStore {
        Self::open(self.mode, &self.temporary, &self.partition, true)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.store.close().expect("fixture engine owned close");
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
