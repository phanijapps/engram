//! Phase-2 code-graph query surface (RFC-0020 / engram-code spec T10):
//! `file_dependencies` and `explore`, composed over the `KnowledgeQuery`
//! port with the pure query functions from `engram-code`. One
//! implementation serves the Rust facade, both MCP servers, and the N-API
//! binding — transports compose, they never reimplement.

use engram_code::{explore as explore_query, file_dependencies as file_deps_query};
use engram_domain::{KnowledgeEntity, Scope};
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

use crate::EngramProvider;

/// One file-level dependency edge (view over `FileDependency`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependencyView {
    pub from_path: String,
    pub import_path: String,
    pub resolved_to: Option<String>,
}

/// One `explore` node (view over `ExploreNode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreNodeView {
    pub name: String,
    pub kind: Option<String>,
    pub hop: u32,
}

impl EngramProvider {
    /// The eligible bound-graph inputs for `scope` (task_015 seam): entities
    /// and relationships constrained to the published generation set BEFORE
    /// any seeding, traversal or analytics, with same-name/same-path nodes
    /// from different bound repositories kept DISTINCT through
    /// repository-qualified node keys (`repo::name`). Returns `None` when no
    /// binding exists — the historical unconstrained methods remain the
    /// pre-binding behavior.
    async fn bound_graph_inputs(
        &self,
        scope: &Scope,
        requested_keys: Option<&[String]>,
    ) -> CoreResult<(
        Vec<KnowledgeEntity>,
        Vec<engram_domain::KnowledgeRelationship>,
        engram_retrieval::BoundCodeView,
    )> {
        let coordinator = self.code_bindings()?;
        let view = coordinator.eligibility_view(scope, requested_keys).await?;
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let mut removed: std::collections::HashSet<String> = Default::default();
        let entities = entities
            .into_iter()
            .filter(|entity| {
                let allowed = view.allows(entity.metadata.as_ref(), entity_unstamped_code(entity));
                if !allowed {
                    removed.insert(entity.id.to_string());
                }
                allowed
            })
            .collect::<Vec<_>>();
        let relationships = relationships
            .into_iter()
            .filter(|relationship| {
                let touches_removed = [
                    relationship.subject.id.as_ref().map(|id| id.as_str()),
                    relationship.object.id.as_ref().map(|id| id.as_str()),
                ]
                .into_iter()
                .flatten()
                .any(|id| removed.contains(id));
                !touches_removed
            })
            .collect::<Vec<_>>();
        Ok((entities, relationships, view))
    }

    /// Repository-qualified node key for an entity: `repo::name` when the
    /// entity carries a bound repository stamp, the bare name otherwise.
    /// Distinct bound repositories never collide — same symbol names and same
    /// paths stay distinct eligible endpoints (task_015 `same_names_cross_repo`).
    fn qualified_name(entity: &KnowledgeEntity) -> String {
        let repo = entity
            .metadata
            .as_ref()
            .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_REPOSITORY_KEY))
            .and_then(|value| value.as_str());
        match repo {
            Some(repo) => format!("{repo}::{}", entity.name),
            None => entity.name.clone(),
        }
    }

    /// [`code_graph_explore_with_artifacts`](Self::code_graph_explore_with_artifacts)
    /// constrained to the published bound generations: seeds, traversal edges
    /// and the `max_nodes`/`max_edges` budgets apply ONLY to the eligible
    /// graph — a stale or unbound node can neither seed, traverse nor consume
    /// budget, and cross-repository edges between two BOUND repositories stay
    /// traversable (task_015).
    pub async fn code_graph_explore_bound(
        &self,
        scope: &Scope,
        query_text: &str,
        depth: Option<u32>,
        max_nodes: Option<usize>,
        max_edges: Option<usize>,
        include_code_artifacts: bool,
    ) -> CoreResult<Vec<ExploreNodeView>> {
        let (entities, relationships, _view) = self.bound_graph_inputs(scope, None).await?;
        let id_to_qualified: std::collections::HashMap<&str, String> = entities
            .iter()
            .map(|entity| (entity.id.as_str(), Self::qualified_name(entity)))
            .collect();
        let names: Vec<(String, Option<String>)> = entities
            .iter()
            .filter(|e| include_code_artifacts || !engram_retrieval::is_code_artifact(e))
            .map(|e| {
                (
                    Self::qualified_name(e),
                    Some(format!("{:?}", e.kind).to_lowercase()),
                )
            })
            .collect();
        // Translate relationship endpoints (entity ids) into the same
        // qualified key space so traversal stays within the eligible graph.
        let qualified_relationships: Vec<engram_domain::KnowledgeRelationship> = relationships
            .iter()
            .map(|relationship| {
                let mut mapped = relationship.clone();
                if let Some(entity_ref) = mapped
                    .subject
                    .id
                    .as_ref()
                    .and_then(|id| id_to_qualified.get(id.as_str()))
                {
                    mapped.subject.name = Some(entity_ref.clone());
                }
                if let Some(entity_ref) = mapped
                    .object
                    .id
                    .as_ref()
                    .and_then(|id| id_to_qualified.get(id.as_str()))
                {
                    mapped.object.name = Some(entity_ref.clone());
                }
                mapped
            })
            .collect();
        Ok(explore_query(
            &qualified_relationships,
            &names,
            query_text,
            depth.unwrap_or(2),
            max_nodes.unwrap_or(24),
            max_edges.unwrap_or(64),
        )
        .into_iter()
        .map(|node| ExploreNodeView {
            name: node.name,
            kind: node.kind,
            hop: node.hop,
        })
        .collect())
    }

    /// [`code_graph_file_dependencies`](Self::code_graph_file_dependencies)
    /// constrained to the published bound generations: import edges survive
    /// exactly when both endpoints are eligible — including authorized
    /// cross-repository edges between two bound repositories — and same-path
    /// files from different bound repositories remain distinct endpoints.
    pub async fn code_graph_file_dependencies_bound(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<FileDependencyView>> {
        let (entities, relationships, _view) = self.bound_graph_inputs(scope, None).await?;
        let id_to_qualified: std::collections::HashMap<&str, String> = entities
            .iter()
            .map(|entity| (entity.id.as_str(), Self::qualified_name(entity)))
            .collect();
        let paths: Vec<String> = entities
            .iter()
            .filter(|e| matches!(e.kind, engram_domain::EntityKind::File))
            .map(Self::qualified_name)
            .collect();
        let qualified_relationships: Vec<engram_domain::KnowledgeRelationship> = relationships
            .iter()
            .map(|relationship| {
                let mut mapped = relationship.clone();
                if let Some(entity_ref) = mapped
                    .subject
                    .id
                    .as_ref()
                    .and_then(|id| id_to_qualified.get(id.as_str()))
                {
                    mapped.subject.name = Some(entity_ref.clone());
                }
                mapped
            })
            .collect();
        Ok(file_deps_query(&qualified_relationships, &paths)
            .into_iter()
            .map(|dependency| FileDependencyView {
                from_path: dependency.from_path,
                import_path: dependency.import_path,
                resolved_to: dependency.resolved_to,
            })
            .collect())
    }

    /// The file-level import graph in `scope`: every scanned file → the
    /// module paths it imports, resolved to defining files by stem suffix.
    pub async fn code_graph_file_dependencies(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<FileDependencyView>> {
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let paths: Vec<String> = entities
            .iter()
            .filter(|e| matches!(e.kind, engram_domain::EntityKind::File))
            .map(|e| e.name.clone())
            .collect();
        Ok(file_deps_query(&relationships, &paths)
            .into_iter()
            .map(|d| FileDependencyView {
                from_path: d.from_path,
                import_path: d.import_path,
                resolved_to: d.resolved_to,
            })
            .collect())
    }

    /// Natural-language explore: identifier-token seeding + bounded
    /// expansion over `calls`/`contains` edges. Defaults per the contract:
    /// depth 2, 24 nodes, 64 edges.
    ///
    /// `include_code_artifacts` (default false): Variable/Module/File/
    /// Repository entities are excluded from the discovery seed list unless
    /// opted in; expansion edges may still traverse them (raw structural
    /// view — matches the MCP `explore` tool's semantics).
    pub async fn code_graph_explore(
        &self,
        scope: &Scope,
        query_text: &str,
        depth: Option<u32>,
        max_nodes: Option<usize>,
        max_edges: Option<usize>,
    ) -> CoreResult<Vec<ExploreNodeView>> {
        self.code_graph_explore_with_artifacts(
            scope, query_text, depth, max_nodes, max_edges, false,
        )
        .await
    }

    /// [`code_graph_explore`](Self::code_graph_explore) with explicit
    /// code-artifact eligibility for the discovery seed list.
    pub async fn code_graph_explore_with_artifacts(
        &self,
        scope: &Scope,
        query_text: &str,
        depth: Option<u32>,
        max_nodes: Option<usize>,
        max_edges: Option<usize>,
        include_code_artifacts: bool,
    ) -> CoreResult<Vec<ExploreNodeView>> {
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let names: Vec<(String, Option<String>)> = entities
            .iter()
            .filter(|e| include_code_artifacts || !engram_retrieval::is_code_artifact(e))
            .map(|e| (e.name.clone(), Some(format!("{:?}", e.kind).to_lowercase())))
            .collect();
        Ok(explore_query(
            &relationships,
            &names,
            query_text,
            depth.unwrap_or(2),
            max_nodes.unwrap_or(24),
            max_edges.unwrap_or(64),
        )
        .into_iter()
        .map(|n| ExploreNodeView {
            name: n.name,
            kind: n.kind,
            hop: n.hop,
        })
        .collect())
    }
}

/// True for code-symbol / code-artifact entities WITHOUT a generation stamp —
/// legacy unbound code, never eligible under a binding view (task_015; shared
/// shape with the retrieval lanes' classifier).
fn entity_unstamped_code(entity: &KnowledgeEntity) -> bool {
    let stamped = entity
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY))
        .is_some();
    if stamped {
        return false;
    }
    engram_retrieval::is_code_artifact(entity)
        || matches!(
            entity.kind,
            engram_domain::EntityKind::Class
                | engram_domain::EntityKind::Function
                | engram_domain::EntityKind::Method
                | engram_domain::EntityKind::Struct
                | engram_domain::EntityKind::Interface
                | engram_domain::EntityKind::Trait
                | engram_domain::EntityKind::TypeAlias
                | engram_domain::EntityKind::Enum
        )
}
