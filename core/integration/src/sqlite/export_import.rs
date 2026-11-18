//! SQLite inventory contribution and compatibility constructor for canonical v1 export.
use crate::{
    ExportImport, ImportData,
    archive_composition::export_snapshot,
    storage_composition::{ArchiveInventory, ArchiveSnapshot},
};
use async_trait::async_trait;
use engram_domain::Scope;
use engram_knowledge::TaxonomyRepository as _;
use engram_runtime::CoreResult;
use engram_store_sqlite::{SqlBeliefStore, SqlHierarchyStore, SqlKnowledgeStore, SqlMemoryService};
use std::sync::Arc;

pub struct SqlExportImport {
    knowledge: Arc<SqlKnowledgeStore>,
    memory: Arc<SqlMemoryService>,
    belief: Option<Arc<SqlBeliefStore>>,
    hierarchy: Option<Arc<SqlHierarchyStore>>,
}

impl SqlExportImport {
    /// Wraps the shared knowledge + memory store handles to expose scope export.
    /// The belief + hierarchy families are not exported until their stores are
    /// attached with [`SqlExportImport::with_belief`] /
    /// [`SqlExportImport::with_hierarchy`].
    pub fn new(knowledge: Arc<SqlKnowledgeStore>, memory: Arc<SqlMemoryService>) -> Self {
        Self {
            knowledge,
            memory,
            belief: None,
            hierarchy: None,
        }
    }

    /// Attaches the belief store so the export reads a scope's beliefs via
    /// [`SqlBeliefStore::list_beliefs`]. Without this call the `beliefs`
    /// family in the exported [`ImportData`] is empty.
    pub fn with_belief(mut self, belief: Arc<SqlBeliefStore>) -> Self {
        self.belief = Some(belief);
        self
    }

    /// Attaches the hierarchy store so the export reads a scope's hierarchy
    /// nodes via [`SqlHierarchyStore::list_nodes`]. Without this call the
    /// `hierarchy_nodes` family in the exported [`ImportData`] is empty.
    pub fn with_hierarchy(mut self, hierarchy: Arc<SqlHierarchyStore>) -> Self {
        self.hierarchy = Some(hierarchy);
        self
    }
}

#[async_trait]
impl ArchiveInventory for SqlExportImport {
    async fn snapshot(&self, scope: &Scope) -> CoreResult<ArchiveSnapshot> {
        let sources = self.knowledge.list_sources(scope).await?;
        let documents = self.knowledge.list_documents(scope).await?;
        let chunks = self.knowledge.list_chunks(scope).await?;
        let entities = self.knowledge.list_entities(scope).await?;
        let relationships = self.knowledge.list_relationships(scope).await?;
        let memories = self.memory.list_memories_in_scope(scope)?;
        let concept_schemes = self.knowledge.list_concept_schemes(scope).await?;
        let mut concepts = Vec::new();
        for scheme in &concept_schemes {
            concepts.extend(
                self.knowledge
                    .list_concepts(&scheme.id, scope)
                    .await?
                    .into_iter()
                    .map(|concept| (concept, scheme.scope.clone())),
            );
        }
        let beliefs = match &self.belief {
            Some(store) => store.list_beliefs(scope).await?,
            None => Vec::new(),
        };
        let hierarchy_nodes = match &self.hierarchy {
            Some(store) => store.list_nodes(scope).await?,
            None => Vec::new(),
        };
        Ok(ArchiveSnapshot {
            memories,
            sources,
            documents,
            chunks,
            entities,
            relationships,
            concept_schemes,
            concepts,
            beliefs,
            hierarchy_nodes,
        })
    }
}
#[async_trait]
impl ExportImport for SqlExportImport {
    async fn export(&self, scope: &Scope) -> CoreResult<ImportData> {
        export_snapshot(self.snapshot(scope).await?, scope)
    }
}
