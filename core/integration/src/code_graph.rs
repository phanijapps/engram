//! Phase-2 code-graph query surface (RFC-0020 / engram-code spec T10):
//! `file_dependencies` and `explore`, composed over the `KnowledgeQuery`
//! port with the pure query functions from `engram-code`. One
//! implementation serves the Rust facade, both MCP servers, and the N-API
//! binding — transports compose, they never reimplement.
//!
//! Corrective r2 (findings 3/8/10): the BOUND variants authenticate the
//! caller through the shared `CodeBindingAuthorizer` (a denying authorizer
//! denies even for populated scopes), resolve the eligible universe inside
//! the coordinator's consistent-read window (a publication race retries once
//! or surfaces `GenerationChanged`), require BOTH edge endpoints to resolve
//! to ids in the authorized vertex set (dangling/name-only endpoints drop
//! before traversal — they can neither surface nodes nor consume budget),
//! key traversal on stable entity ids with repository-qualified display
//! labels (same names/paths stay distinct), and preserve scanner-resolved
//! import endpoints through dependency composition.

use engram_code::{
    explore_by_id as explore_query_by_id, file_dependencies_resolved as file_deps_resolved_query,
};
use engram_domain::{Actor, KnowledgeEntity, Scope};
use engram_runtime::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::EngramProvider;

/// How long a bound graph read pins its consistent-read window.
const GRAPH_READ_WINDOW: Duration = Duration::from_secs(30);

/// One file-level dependency edge (view over `FileDependency`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependencyView {
    pub from_path: String,
    pub import_path: String,
    pub resolved_to: Option<String>,
}

/// One `explore` node (view over `ExploreNodeById`): the STABLE entity id
/// plus its display label, so same-named symbols never collapse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreNodeView {
    pub id: String,
    pub label: String,
    pub kind: Option<String>,
    pub hop: u32,
}

/// True for symbol / structural code-artifact entity kinds (classification
/// input; durable lineage is the code authority — finding 4).
fn entity_is_code_kind(entity: &KnowledgeEntity) -> bool {
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

/// Repository-qualified display label: `repo::label` when the entity is a
/// durable-lineage code record of that repository, the bare label otherwise.
/// Labels are DISPLAY ONLY — traversal keys on ids (finding 10).
fn display_label(entity: &KnowledgeEntity, lineage_repo: Option<&str>) -> String {
    match lineage_repo {
        Some(repo) => format!("{repo}::{}", entity.name),
        None => entity.name.clone(),
    }
}

impl EngramProvider {
    /// The historical unconstrained file-level import graph (pre-binding
    /// behavior; kept for non-code scopes).
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
        Ok(engram_code::file_dependencies(&relationships, &paths)
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
    /// code-artifact eligibility for the discovery seed list (historical,
    /// unconstrained).
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
        let nodes: Vec<(String, Option<String>)> = entities
            .iter()
            .filter(|e| include_code_artifacts || !engram_retrieval::is_code_artifact(e))
            .map(|e| (e.name.clone(), Some(format!("{:?}", e.kind).to_lowercase())))
            .collect();
        Ok(engram_code::explore(
            &relationships,
            &nodes,
            query_text,
            depth.unwrap_or(2),
            max_nodes.unwrap_or(24),
            max_edges.unwrap_or(64),
        )
        .into_iter()
        .map(|n| ExploreNodeView {
            id: n.name.clone(),
            label: n.name,
            kind: n.kind,
            hop: n.hop,
        })
        .collect())
    }

    /// The authorized bound-graph universe for `scope` (corrective r2): the
    /// durable-lineage view constrains entities BEFORE any seeding or
    /// traversal, and an edge survives only when BOTH endpoints resolve to
    /// ids in the authorized vertex set (finding 8).
    async fn bound_graph_universe(
        &self,
        scope: &Scope,
        requested_keys: Option<&[String]>,
    ) -> CoreResult<BoundGraphUniverse> {
        let query = self.require_knowledge_query()?;
        let relationships = query.list_relationships(scope).await?;
        let entities = query.list_entities(scope).await?;
        let coordinator = self.code_bindings()?;
        let view = coordinator.eligibility_view(scope, requested_keys).await?;
        let mut universe = BoundGraphUniverse::default();
        for entity in &entities {
            if !view.allows(entity.id.as_str(), entity_is_code_kind(entity)) {
                continue;
            }
            universe.authorized_ids.insert(entity.id.to_string());
            let repo = view.code_repo_of(entity.id.as_str()).map(str::to_owned);
            let label = display_label(entity, repo.as_deref());
            universe.nodes.push((
                entity.id.to_string(),
                entity.name.clone(),
                Some(format!("{:?}", entity.kind).to_lowercase()),
            ));
            universe.labels.insert(entity.id.to_string(), label);
        }
        universe.edges = relationships
            .iter()
            .filter_map(|relationship| {
                let subject = relationship.subject.id.as_ref()?;
                let object = relationship.object.id.as_ref()?;
                if universe.authorized_ids.contains(subject.as_str())
                    && universe.authorized_ids.contains(object.as_str())
                {
                    Some((
                        subject.to_string(),
                        object.to_string(),
                        relationship.predicate.clone(),
                    ))
                } else {
                    // Dangling / name-only / unauthorized endpoint: dropped
                    // BEFORE traversal — it can neither surface a node nor
                    // consume budget (finding 8).
                    None
                }
            })
            .collect();
        universe.file_paths = entities
            .iter()
            .filter(|entity| {
                matches!(entity.kind, engram_domain::EntityKind::File)
                    && universe.authorized_ids.contains(entity.id.as_str())
            })
            .map(|entity| {
                let canonical = universe
                    .labels
                    .get(entity.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| entity.name.clone());
                (entity.id.to_string(), canonical)
            })
            .collect();
        Ok(universe)
    }

    /// Authorize `actor` for the anchor repository and EVERY repository in
    /// the (narrowed) eligible set (finding 3): a denying authorizer denies
    /// even for a fully populated scope.
    async fn authorize_bound_graph(
        &self,
        actor: &Actor,
        scope: &Scope,
    ) -> CoreResult<engram_domain::RepoId> {
        let coordinator = self.code_bindings()?;
        let view = coordinator.eligibility_view(scope, None).await?;
        let anchor = view
            .eligible_bindings()
            .first()
            .map(|binding| engram_domain::RepoId(binding.repo_id.clone()));
        let Some(anchor) = anchor else {
            return Err(CoreError::InvalidRequest {
                reason: "BindingRequired: no published bound repository in this scope".into(),
            });
        };
        coordinator.authorize(actor, scope, &anchor).await?;
        for binding in view.eligible_bindings() {
            coordinator
                .authorize(
                    actor,
                    scope,
                    &engram_domain::RepoId(binding.repo_id.clone()),
                )
                .await?;
        }
        Ok(anchor)
    }

    /// Bound explore (corrective r2 findings 3/5/8/10): the caller is
    /// authorized through the shared `CodeBindingAuthorizer`, the eligible
    /// universe (durable lineage + request narrowing) is resolved inside a
    /// consistent-read window (publication race ⇒ retry once or
    /// `GenerationChanged`), traversal keys on stable entity ids with
    /// repository-qualified labels, and node/edge budgets apply ONLY to the
    /// authorized graph.
    #[allow(clippy::double_must_use)]
    pub async fn code_graph_explore_bound(
        &self,
        actor: &Actor,
        scope: &Scope,
        query_text: &str,
        depth: Option<u32>,
        max_nodes: Option<usize>,
        max_edges: Option<usize>,
        include_code_artifacts: bool,
    ) -> CoreResult<Vec<ExploreNodeView>> {
        let anchor = self.authorize_bound_graph(actor, scope).await?;
        let coordinator = self.code_bindings()?.clone();
        let query_text = query_text.to_owned();
        let depth = depth.unwrap_or(2);
        let max_nodes = max_nodes.unwrap_or(24);
        let max_edges = max_edges.unwrap_or(64);
        let universe = self.bound_graph_universe(scope, None).await?;
        // Artifact suppression changes ONLY discovery seeds.  The full
        // authorized universe remains available to traversal so a symbol can
        // legitimately cross a File or Module vertex to another symbol.
        let seed_nodes = universe
            .nodes
            .iter()
            .filter(|(_, _, kind)| {
                include_code_artifacts
                    || !matches!(
                        kind.as_deref(),
                        Some("file") | Some("module") | Some("package")
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        let universe_labels = universe.labels.clone();
        let query_seed = query_text.clone();
        coordinator
            .consistent_read(
                actor,
                scope,
                &anchor,
                GRAPH_READ_WINDOW,
                move |_eligibility| {
                    let universe_labels = universe_labels.clone();
                    let seed_nodes = seed_nodes.clone();
                    let query_text = query_seed.clone();
                    async move {
                        let universe = self.bound_graph_universe(scope, None).await?;
                        let edge_refs: Vec<(&str, &str, &str)> = universe
                            .edges
                            .iter()
                            .map(|(s, o, p)| (s.as_str(), o.as_str(), p.as_str()))
                            .collect();
                        let nodes = seed_nodes.clone();
                        let out = explore_query_by_id(
                            &edge_refs,
                            &nodes,
                            &query_text,
                            depth,
                            max_nodes,
                            max_edges,
                        );
                        Ok(out
                            .into_iter()
                            .map(|node| {
                                let label = universe_labels
                                    .get(&node.id)
                                    .cloned()
                                    .unwrap_or(node.label.clone());
                                ExploreNodeView {
                                    id: node.id,
                                    label,
                                    kind: node.kind,
                                    hop: node.hop,
                                }
                            })
                            .collect())
                    }
                },
            )
            .await
    }

    /// Bound file dependencies (corrective r2 findings 3/8/10): authorized
    /// universe + both-endpoint edge requirement + scanner-resolved import
    /// endpoints preserved through composition (identical paths across
    /// repositories resolve through the scanner's endpoint id, never through
    /// ambiguous suffix re-resolution).
    #[allow(clippy::double_must_use)]
    pub async fn code_graph_file_dependencies_bound(
        &self,
        actor: &Actor,
        scope: &Scope,
    ) -> CoreResult<Vec<FileDependencyView>> {
        let anchor = self.authorize_bound_graph(actor, scope).await?;
        let coordinator = self.code_bindings()?.clone();
        coordinator
            .consistent_read(
                actor,
                scope,
                &anchor,
                GRAPH_READ_WINDOW,
                move |_eligibility| {
                    async move {
                        let universe = self.bound_graph_universe(scope, None).await?;
                        // Canonical id→path map over AUTHORIZED file entities
                        // only: an unauthorized or dangling import target never
                        // resolves (finding 8).
                        let object_id_to_path: std::collections::HashMap<String, String> =
                            universe.file_paths.iter().cloned().collect();
                        let file_paths: Vec<String> = object_id_to_path.values().cloned().collect();
                        // Rebuild the relationship shape with authorized
                        // endpoints and canonical subject labels so the pure
                        // function sees exactly the authorized graph.
                        let query = self.require_knowledge_query()?;
                        let relationships = query.list_relationships(scope).await?;
                        let mut canonical: Vec<engram_domain::KnowledgeRelationship> = Vec::new();
                        for mut relationship in relationships {
                            let (Some(subject), Some(object)) = (
                                relationship.subject.id.clone(),
                                relationship.object.id.clone(),
                            ) else {
                                continue;
                            };
                            if !universe.authorized_ids.contains(subject.as_str())
                                || !universe.authorized_ids.contains(object.as_str())
                            {
                                continue;
                            }
                            if let Some(label) = universe.labels.get(subject.as_str()) {
                                relationship.subject.name = Some(label.clone());
                            }
                            if let Some(label) = universe.labels.get(object.as_str()) {
                                relationship.object.name = Some(label.clone());
                            }
                            canonical.push(relationship);
                        }
                        Ok(
                            file_deps_resolved_query(&canonical, &file_paths, &object_id_to_path)
                                .into_iter()
                                .map(|dependency| FileDependencyView {
                                    from_path: dependency.from_path,
                                    import_path: dependency.import_path,
                                    resolved_to: dependency.resolved_to,
                                })
                                .collect(),
                        )
                    }
                },
            )
            .await
    }
}

/// The authorized bound-graph universe (entities + both-endpoint-authorized
/// edges + canonical file paths), built once per read.
#[derive(Default)]
struct BoundGraphUniverse {
    authorized_ids: std::collections::HashSet<String>,
    /// (entity id, raw label, kind) — traversal keys are the IDS.
    nodes: Vec<(String, String, Option<String>)>,
    edges: Vec<(String, String, String)>,
    /// entity id → canonical (repository-qualified) display label.
    labels: std::collections::HashMap<String, String>,
    /// (file entity id → canonical path) for authorized FILE entities.
    file_paths: Vec<(String, String)>,
}
