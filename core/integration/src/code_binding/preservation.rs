//! Complete preserved-knowledge inventory used by the reset coordinator.
use super::CodeBindingCoordinator;
use async_trait::async_trait;
use engram_domain::{PreservationInventory, PreservationRecord, Scope};
use engram_knowledge::{CodeBindingRepository, KnowledgeRepository};
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Recipe-injected read boundary for records reset must not mutate.
///
/// It deliberately belongs to the coordinator rather than the binding cell because
/// the binding cell has no ownership of the knowledge store.
#[async_trait]
pub trait PreservationRecordSource: Send + Sync {
    async fn inventory(&self, scope: &Scope) -> CoreResult<PreservationInventory>;
}

pub(crate) struct UnavailablePreservationRecordSource;
#[async_trait]
impl PreservationRecordSource for UnavailablePreservationRecordSource {
    async fn inventory(&self, _scope: &Scope) -> CoreResult<PreservationInventory> {
        Err(CoreError::CapabilityUnsupported {
            capability: "PreservationSourceUnavailable".into(),
            reason: "reset requires a recipe-injected knowledge preservation source".into(),
        })
    }
}

/// Knowledge-store implementation using existing repository/query enumeration ports.
pub struct KnowledgePreservationRecordSource {
    knowledge: Arc<dyn KnowledgeRepository>,
    query: Arc<dyn crate::KnowledgeQuery>,
    bindings: Arc<dyn CodeBindingRepository>,
}
impl KnowledgePreservationRecordSource {
    pub fn new(
        knowledge: Arc<dyn KnowledgeRepository>,
        query: Arc<dyn crate::KnowledgeQuery>,
        bindings: Arc<dyn CodeBindingRepository>,
    ) -> Self {
        Self {
            knowledge,
            query,
            bindings,
        }
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
impl KnowledgePreservationRecordSource {
    async fn entry<T: serde::Serialize>(
        &self,
        class: &str,
        id: String,
        value: &T,
        scope: &Scope,
    ) -> CoreResult<PreservationRecord> {
        let content = serde_json::to_vec(value).map_err(|_| CoreError::Adapter {
            adapter: "engram-preservation".into(),
            message: "cannot serialize knowledge record".into(),
        })?;
        let contributor_digest = match self.bindings.record_lineage(scope, &id).await? {
            Some(lineage) => digest(&serde_json::to_vec(&lineage.contributors).map_err(|_| {
                CoreError::Adapter {
                    adapter: "engram-preservation".into(),
                    message: "cannot serialize contributors".into(),
                }
            })?),
            // A fixed digest distinguishes no durable lineage from a lineage-bearing record.
            None => digest(b"[]"),
        };
        Ok(PreservationRecord {
            record_class: class.into(),
            record_id: id,
            content_digest: digest(&content),
            contributor_digest,
        })
    }
}
#[async_trait]
impl PreservationRecordSource for KnowledgePreservationRecordSource {
    async fn inventory(&self, scope: &Scope) -> CoreResult<PreservationInventory> {
        let sources = self.knowledge.list_sources(scope).await?;
        let documents = self.knowledge.list_documents(scope).await?;
        let chunks = self.query.list_chunks(scope).await?;
        let entities = self.query.list_entities(scope).await?;
        let relationships = self.query.list_relationships(scope).await?;
        let graphs = self.query.list_graphs(scope).await?;
        let mut records = Vec::new();
        for value in sources {
            records.push(
                self.entry("source", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        for value in documents {
            records.push(
                self.entry("document", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        for value in chunks {
            records.push(
                self.entry("chunk", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        for value in entities {
            records.push(
                self.entry("entity", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        for value in relationships {
            records.push(
                self.entry("relationship", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        for value in graphs {
            records.push(
                self.entry("graph", value.id.to_string(), &value, scope)
                    .await?,
            );
        }
        records.sort_by(|left, right| {
            (&left.record_class, &left.record_id).cmp(&(&right.record_class, &right.record_id))
        });
        let mut inventory = PreservationInventory {
            records,
            digest: String::new(),
        };
        inventory.digest = inventory.content_hash().map_err(|_| CoreError::Adapter {
            adapter: "engram-preservation".into(),
            message: "cannot hash preservation inventory".into(),
        })?;
        Ok(inventory)
    }
}

impl CodeBindingCoordinator {
    /// Injects the recipe-owned knowledge reader required for reset preservation.
    pub fn with_preservation_source(mut self, source: Arc<dyn PreservationRecordSource>) -> Self {
        self.preservation_source = source;
        self
    }
}
