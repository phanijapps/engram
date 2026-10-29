//! Engine-neutral query port for listing knowledge-graph records in a scope.
//!
//! [`KnowledgeRepository`](engram_knowledge::KnowledgeRepository) and
//! [`KnowledgeGraphRepository`](engram_knowledge::KnowledgeGraphRepository) are
//! write/lookup ports (put / get / delete / neighbors); they intentionally
//! expose no "list everything in scope" method. Code-intelligence composition —
//! and any caller that needs the full entity/edge set for a project — goes
//! through this port so it can route through the
//! [`EngramProvider`](crate::EngramProvider) instead of reaching into a concrete
//! store (the old `codegraph/mcp-server` reached into the concrete store
//! directly; this port removes that need).

use async_trait::async_trait;
use engram_domain::{
    ChunkId, DocumentId, KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeRelationship,
    Scope,
};
use engram_runtime::{CoreError, CoreResult};
use std::sync::Arc;
use std::time::Duration;

const AUTHORIZED_QUERY_WINDOW: Duration = Duration::from_secs(30);

/// Read port: list the entities / relationships / chunks visible to a scope.
/// Lean chunk reference for embed/index paths (see
/// [`KnowledgeQuery::list_chunk_refs`]).
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub id: ChunkId,
    /// The chunk's provenance source string (scan delta filtering).
    pub source: String,
    /// `false` for empty-text chunks — excluded from embedding without
    /// needing the text itself.
    pub has_text: bool,
    /// The chunk's text content hash (scanner-stamped, full-text domain) —
    /// the durable-dedup key: two chunks with identical text share it.
    pub content_hash: String,
}

#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait KnowledgeQuery: Send + Sync {
    /// All entities in `scope`.
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;

    /// All relationships in `scope`.
    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;

    /// All chunks in `scope` (for embedding/indexing). Default: empty (not supported).
    async fn list_chunks(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }

    /// Lean chunk listing for embed/index paths: id + provenance source +
    /// text-nonemptiness per chunk in scope — no text transfer, no full
    /// record deserialization. The embed pending-set computation needs
    /// exactly this; materializing full chunk records (text included) for
    /// 90k+ chunks cost ~1.4s and hundreds of MB per call. Default: derived
    /// from [`Self::list_chunks`] (correct everywhere, efficient where the
    /// store overrides it).
    async fn list_chunk_refs(&self, scope: &Scope) -> CoreResult<Vec<ChunkRef>> {
        Ok(self
            .list_chunks(scope)
            .await?
            .into_iter()
            .map(|c| ChunkRef {
                id: c.id,
                source: c.provenance.source,
                has_text: !c.text.is_empty(),
                content_hash: c.content_hash,
            })
            .collect())
    }

    /// All graphs in `scope`. Default: empty (not supported).
    async fn list_graphs(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        Ok(Vec::new())
    }

    /// The chunks of one `document_id` within `scope` (bounded per-document read,
    /// used by the extract-knowledge op). Default: empty (not supported).
    async fn list_chunks_by_document(
        &self,
        _document_id: &DocumentId,
        _scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        Ok(Vec::new())
    }
}

/// A validated maintenance grant (task_016 r3): authorization + canonical
/// target selection ran under one pinned consistent-read window. The grant
/// records the authorizing caller, scope, targets and the authorized view's
/// epoch. Possession alone authorizes nothing — every use revalidates.
#[derive(Clone, Debug)]
pub struct MaintenanceTarget {
    /// The canonical scope/repository/generation association selected under the grant view.
    scope: Scope,
    repo_id: engram_domain::RepoId,
    generation_id: String,
    record_id: String,
}

/// Typed owning-store maintenance operation.
///
/// Production callers cannot supply arbitrary callbacks: retirement is the sole
/// supported operation and executes inside the binding store serialization domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaintenanceOperation {
    RetireLineageTargets,
}

pub struct MaintenanceGrant {
    scope: Scope,
    actor_id: String,
    targets: Vec<MaintenanceTarget>,
    epoch: u64,
    /// Per-repository BINDING epochs captured at authorization — the CAS
    /// values the owned retirement fence checks (corrective r4, canonical
    /// 3a). The scope `epoch` stays as a fast pre-check only.
    binding_epochs: Vec<(engram_domain::RepoId, u64)>,
}

impl MaintenanceGrant {
    /// The authorized maintenance targets (pinned at authorization).
    pub fn targets(&self) -> Vec<String> {
        self.targets
            .iter()
            .map(|target| target.record_id.clone())
            .collect()
    }

    /// The eligibility epoch the grant was authorized against.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The per-repo binding epochs pinned at authorization (fence CAS input).
    pub fn binding_epochs(&self) -> &[(engram_domain::RepoId, u64)] {
        &self.binding_epochs
    }

    /// The typed retirement associations (repository + EXACT generation per
    /// record) the store fence validates (review r5, finding 3).
    pub fn retirement_targets(&self) -> Vec<engram_knowledge::RetirementTarget> {
        self.targets
            .iter()
            .map(|target| engram_knowledge::RetirementTarget {
                repo_id: target.repo_id.0.clone(),
                generation_id: target.generation_id.clone(),
                record_id: target.record_id.clone(),
            })
            .collect()
    }
}

/// Shared scoped authorization for explicit-ID code reads, wildcard listing
/// and maintenance mutations (task_016 + corrective r2 findings 3/4,
/// engine-neutral).
///
/// One seam wraps one [`KnowledgeQuery`] and the binding COORDINATOR — so
/// every read/maintenance call authenticates the caller through the shared
/// `CodeBindingAuthorizer` (a denying authorizer denies even for fully
/// populated foreign scopes) and classifies targets by DURABLE LINEAGE +
/// record kind (metadata is never authority):
/// - **Explicit code ids** must resolve to lineage of a PUBLISHED generation
///   in the request scope; a foreign, staged or unknown code id denies the
///   whole request (fail closed — never a silent partial result).
/// - **Explicit ordinary non-code ids** stay readable: an authorized concept
///   or document entity does not need code lineage (finding 4).
/// - **Wildcard / empty request** resolves to exactly the published set —
///   never staged, foreign or unbound repositories, and unstamped legacy
///   CODE kinds are excluded from wildcard listings.
/// - **Maintenance** requires explicit authorized CODE ids; there is no
///   scope-wide maintenance shortcut.
pub struct AuthorizedKnowledgeQuery {
    query: Arc<dyn KnowledgeQuery>,
    coordinator: Arc<crate::code_binding::CodeBindingCoordinator>,
    /// Owning ordinary-record policy authorizer for scopes with NO published
    /// binding (corrective r4, canonical 3c/4): ordinary records still need
    /// their own caller authorization, never an invented repository anchor.
    policy_authorizer: Option<Arc<dyn engram_runtime::PolicyAuthorizer>>,
    graph: Option<Arc<dyn engram_knowledge::KnowledgeGraphRepository>>,
}

impl AuthorizedKnowledgeQuery {
    /// Wraps a knowledge read surface and the binding coordinator (the
    /// coordinator owns both the authorizer and the durable store).
    pub fn new(
        query: Arc<dyn KnowledgeQuery>,
        coordinator: Arc<crate::code_binding::CodeBindingCoordinator>,
    ) -> Self {
        Self {
            query,
            coordinator,
            policy_authorizer: None,
            graph: None,
        }
    }

    /// Attaches the owning ordinary-record policy authorizer (unbound-scope
    /// branch). Without it, an unbound scope cannot authorize ordinary
    /// reads and fails closed.
    pub fn with_policy_authorizer(
        mut self,
        authorizer: Arc<dyn engram_runtime::PolicyAuthorizer>,
    ) -> Self {
        self.policy_authorizer = Some(authorizer);
        self
    }

    /// Attaches the canonical graph repository for graph-owned ordinary records.
    pub fn with_graph_repository(
        mut self,
        graph: Arc<dyn engram_knowledge::KnowledgeGraphRepository>,
    ) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Authorizes one ordinary entity through its canonical graph policy, or
    /// through the domain graphless default when it has no graph association.
    /// A missing authorizer, graph repository, graph row, or foreign graph
    /// fails closed.
    async fn ordinary_authorized(
        &self,
        actor: &engram_domain::Actor,
        entity: &KnowledgeEntity,
    ) -> CoreResult<bool> {
        ordinary_entity_authorized(&self.policy_authorizer, &self.graph, actor, entity).await
    }

    /// Authorizes the caller for every repository in the scope's published
    /// set (finding 3): a denying authorizer denies even for populated
    /// foreign scopes.
    async fn authorize_caller(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
    ) -> CoreResult<engram_domain::RepoId> {
        let view = self.coordinator.eligibility_view(scope, None).await?;
        let anchor = view
            .eligible_bindings()
            .first()
            .map(|binding| engram_domain::RepoId(binding.repo_id.clone()))
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "BindingRequired: no published bound repository in this scope".into(),
            })?;
        for binding in view.eligible_bindings() {
            self.coordinator
                .authorize(
                    actor,
                    scope,
                    &engram_domain::RepoId(binding.repo_id.clone()),
                )
                .await?;
        }
        Ok(anchor)
    }

    /// Authorizes an explicit-id (or wildcard, when `requested` is empty)
    /// read and returns the constrained target set: published code lineage
    /// targets only for code ids; explicitly requested ordinary non-code
    /// records pass through as authorized non-code (finding 4).
    #[allow(clippy::double_must_use)]
    pub async fn authorize_read(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
        limit: usize,
    ) -> CoreResult<Vec<String>> {
        let _anchor = self.authorize_caller(actor, scope).await?;
        let view = self.coordinator.eligibility_view(scope, None).await?;
        if requested.is_empty() {
            // Wildcard: exactly the eligible code targets (durable lineage of
            // repo-selected published bindings), bounded by the budget. It
            // can never widen to staged, foreign or unbound repositories.
            return Ok(eligible_lineage_records(&view, limit));
        }
        // Explicit ids: classify each. Code ids (durable lineage) must be
        // eligible; ordinary non-code records are authorized non-code reads;
        // unknown ids deny the whole request.
        let mut authorized = Vec::with_capacity(requested.len());
        for id in requested {
            if view.is_code_record(id) {
                if !view.allows(id, true) {
                    return Err(foreign_target(&[id.clone()]));
                }
                authorized.push(id.clone());
            } else {
                // Ordinary non-code record? It must actually exist as a
                // non-code record in the read surface — otherwise it is an
                // unknown id and denies.
                if let Some(entity) = await_non_code_entity(&self.query, scope, id).await? {
                    if self.ordinary_authorized(actor, &entity).await? {
                        authorized.push(id.clone());
                    } else {
                        return Err(foreign_target(&[id.clone()]));
                    }
                } else {
                    return Err(foreign_target(&[id.clone()]));
                }
            }
        }
        Ok(authorized)
    }

    /// Authorizes a maintenance mutation over explicit target ids — the same
    /// authorization view, never a wider scope. Maintenance targets must be
    /// ELIGIBLE CODE ids (durable lineage of published bindings).
    #[allow(clippy::double_must_use)]
    pub async fn authorize_maintenance(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<String>> {
        if requested.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "maintenance authorization requires explicit target ids".into(),
            });
        }
        let anchor = self.authorize_caller(actor, scope).await?;
        let requested = requested.to_vec();
        let coordinator = Arc::clone(&self.coordinator);
        let view_coordinator = Arc::clone(&coordinator);
        // Return an authority token only from a pinned view.  A maintenance
        // caller must consume this result before the same consistent-read
        // window closes; a publication race retries or denies instead of
        // handing out IDs selected under an older epoch.
        coordinator
            .consistent_read(
                actor,
                scope,
                &anchor,
                AUTHORIZED_QUERY_WINDOW,
                move |_lease| {
                    let coordinator = Arc::clone(&view_coordinator);
                    let requested = requested.clone();
                    async move {
                        let view = coordinator.eligibility_view(scope, None).await?;
                        let mut authorized = Vec::with_capacity(requested.len());
                        for id in &requested {
                            if !view.is_code_record(id) || !view.allows(id, true) {
                                return Err(foreign_target(&[id.clone()]));
                            }
                            authorized.push(id.clone());
                        }
                        Ok(authorized)
                    }
                },
            )
            .await
    }

    /// Authorizes maintenance and returns a PINNED grant bound to the
    /// authorized view's epoch (task_016 r3). The grant is NOT authority by
    /// possession: returning authorized ids from `consistent_read` never
    /// authorizes a later mutation — pair the grant with
    /// [`Self::use_maintenance`], which revalidates before any owned
    /// mutation runs.
    #[allow(clippy::double_must_use)]
    pub async fn authorize_maintenance_grant(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<MaintenanceGrant> {
        if requested.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "maintenance authorization requires explicit target ids".into(),
            });
        }
        let anchor = self.authorize_caller(actor, scope).await?;
        let requested = requested.to_vec();
        let actor_id = actor.id.to_string();
        let coordinator = Arc::clone(&self.coordinator);
        let view_coordinator = Arc::clone(&coordinator);
        coordinator
            .consistent_read(
                actor,
                scope,
                &anchor,
                AUTHORIZED_QUERY_WINDOW,
                move |eligibility| {
                    let coordinator = Arc::clone(&view_coordinator);
                    let requested = requested.clone();
                    let actor_id = actor_id.clone();
                    async move {
                        let view = coordinator.eligibility_view(scope, None).await?;
                        let mut authorized = Vec::with_capacity(requested.len());
                        for id in &requested {
                            if !view.is_code_record(id) || !view.allows(id, true) {
                                return Err(foreign_target(&[id.clone()]));
                            }
                            authorized.push(id.clone());
                        }
                        // Pin every published repository's BINDING epoch: the
                        // owned retirement fence CASes these values.
                        let mut binding_epochs = Vec::new();
                        for binding in view.eligible_bindings() {
                            let repo = engram_domain::RepoId(binding.repo_id.clone());
                            let Some(binding_record) =
                                coordinator.store().get_binding(scope, &repo).await?
                            else {
                                return Err(foreign_target(&[binding.repo_id.clone()]));
                            };
                            binding_epochs.push((repo, binding_record.epoch));
                        }
                        let lineage = coordinator.store().target_lineage(&scope).await?;
                        let targets = authorized
                            .iter()
                            .map(|record_id| {
                                let entry = lineage
                                    .iter()
                                    .find(|entry| entry.record_id == *record_id && !entry.retired)
                                    .ok_or_else(|| foreign_target(&[record_id.clone()]))?;
                                Ok(MaintenanceTarget {
                                    scope: scope.clone(),
                                    repo_id: engram_domain::RepoId(entry.repo_id.clone()),
                                    generation_id: entry.generation_id.clone(),
                                    record_id: record_id.clone(),
                                })
                            })
                            .collect::<CoreResult<Vec<_>>>()?;
                        Ok(MaintenanceGrant {
                            scope: scope.clone(),
                            actor_id,
                            targets,
                            epoch: eligibility.epoch,
                            binding_epochs,
                        })
                    }
                },
            )
            .await
    }

    /// Executes a typed owning-store operation after revalidating its pinned
    /// scope, repository/generation associations, caller, and binding epochs.
    /// The store performs retirement and epoch comparison in one transaction.
    #[allow(clippy::double_must_use)]
    pub async fn use_maintenance(
        &self,
        actor: &engram_domain::Actor,
        grant: &MaintenanceGrant,
        operation: MaintenanceOperation,
    ) -> CoreResult<()> {
        if grant.actor_id != actor.id.to_string() {
            return Err(CoreError::InvalidRequest {
                reason:
                    "maintenance grant caller mismatch: a grant is bound to its authorizing caller"
                        .into(),
            });
        }
        let anchor = self.authorize_caller(actor, &grant.scope).await?;
        let epoch = grant.epoch;
        let targets = grant.targets.clone();
        let coordinator = Arc::clone(&self.coordinator);
        let view_coordinator = Arc::clone(&coordinator);
        coordinator.consistent_read(actor, &grant.scope, &anchor, AUTHORIZED_QUERY_WINDOW, |eligibility| {
            let coordinator = Arc::clone(&view_coordinator);
            let targets = targets.clone();
            let scope = grant.scope.clone();
            async move {
                if eligibility.epoch != epoch {
                    return Err(CoreError::Conflict { reason: "GenerationChanged: maintenance grant is stale — re-authorize against the current view".into() });
                }
                let view = coordinator.eligibility_view(&scope, None).await?;
                let lineage = coordinator.store().target_lineage(&scope).await?;
                for target in &targets {
                    if target.scope != scope || !view.allows(&target.record_id, true)
                        || !lineage.iter().any(|entry| !entry.retired && entry.record_id == target.record_id && entry.repo_id == target.repo_id.0 && entry.generation_id == target.generation_id) {
                        return Err(foreign_target(&[target.record_id.clone()]));
                    }
                }
                Ok(())
            }
        }).await?;
        match operation {
            MaintenanceOperation::RetireLineageTargets => {
                let targets = grant.retirement_targets();
                coordinator
                    .retire_maintenance_targets(
                        actor,
                        &grant.scope,
                        &grant.binding_epochs,
                        &targets,
                    )
                    .await
            }
        }
    }

    /// The positively authorized ORDINARY subset of a scope's entities
    /// (review r5, finding 8): every code-owned record (durable lineage,
    /// RETIRED records included — finding 2) and every code-kind record is
    /// withheld, and each surviving ordinary entity is authorized through
    /// its OWN graph policy or the graphless lane default via the configured
    /// authorizer. Used for unbound scopes AND for populated scopes whose
    /// code authority denies the caller — a code denial must never erase the
    /// independently authorized ordinary subset.
    async fn ordinary_entities_authorized(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
        view: &engram_retrieval::BoundCodeView,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let code_owned: std::collections::HashSet<String> =
            view.code_records().into_iter().map(str::to_owned).collect();
        let entities = self.query.list_entities(scope).await?;
        let mut ordinary = Vec::new();
        for entity in entities {
            if !entity_is_code_kind(&entity)
                && !code_owned.contains(entity.id.as_str())
                && self.ordinary_authorized(actor, &entity).await?
            {
                ordinary.push(entity);
            }
        }
        if requested.is_empty() {
            return Ok(ordinary);
        }
        let requested: std::collections::HashSet<&str> =
            requested.iter().map(String::as_str).collect();
        let selected: Vec<KnowledgeEntity> = ordinary
            .into_iter()
            .filter(|entity| requested.contains(entity.id.as_str()))
            .collect();
        if selected.len() != requested.len() {
            return Err(CoreError::InvalidRequest {
                reason: "BindingRequired: code target ids are foreign or unpublished in this scope"
                    .into(),
            });
        }
        Ok(selected)
    }

    /// Entities visible to `scope`, with code-derived entities constrained to
    /// the authorized target set: non-code entities always flow (AC-007),
    /// and unstamped legacy CODE kinds never appear in wildcard listings
    /// (finding 4).
    #[allow(clippy::double_must_use)]
    pub async fn list_entities_authorized(
        &self,
        actor: &engram_domain::Actor,
        scope: &Scope,
        requested: &[String],
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let initial_view = self.coordinator.eligibility_view(scope, None).await?;
        if initial_view.eligible_bindings().is_empty() {
            // Corrective r4 (canonical 3c/4): no published binding exists, so
            // there is no repository authority to pin — but ordinary records
            // still need their OWN caller authorization (never an invented
            // anchor), and durable lineage still classifies code ownership:
            // a staged code-owned record denies even when its kind is
            // ordinary-looking (e.g. a Concept contributed by a staged
            // attempt), and every code-kind record denies.
            return self
                .ordinary_entities_authorized(actor, scope, requested, &initial_view)
                .await;
        }
        // Review r5 (finding 8): ordinary per-record authorization is
        // SEPARATE from code authorization. When the caller cannot inspect
        // the scope's published code repositories, the code entities are
        // withheld but the positively authorized ordinary subset still
        // flows through its own graph policies — the code denial is not an
        // ordinary-record denial.
        let anchor = match self.authorize_caller(actor, scope).await {
            Ok(anchor) => anchor,
            Err(CoreError::PolicyDenied { .. }) => {
                return self
                    .ordinary_entities_authorized(actor, scope, requested, &initial_view)
                    .await;
            }
            Err(error) => return Err(error),
        };
        let requested = requested.to_vec();
        let query = Arc::clone(&self.query);
        let coordinator = Arc::clone(&self.coordinator);
        let view_coordinator = Arc::clone(&coordinator);
        let policy_authorizer = self.policy_authorizer.clone();
        let graph = self.graph.clone();
        let caller = actor.clone();
        // Pin authority, canonical selection, entity load, and returned view
        // into one consistency window. A publication race retries once or
        // returns GenerationChanged before stale results can be returned.
        coordinator
            .consistent_read(
                actor,
                scope,
                &anchor,
                AUTHORIZED_QUERY_WINDOW,
                move |_lease| {
                    let query = Arc::clone(&query);
                    let coordinator = Arc::clone(&view_coordinator);
                    let requested = requested.clone();
                    let policy_authorizer = policy_authorizer.clone();
                    let graph = graph.clone();
                    let caller = caller.clone();
                    async move {
                        let view = coordinator.eligibility_view(scope, None).await?;
                        let entities = query.list_entities(scope).await?;
                        if requested.is_empty() {
                            let mut authorized = Vec::new();
                            for entity in entities {
                                if view.is_code_record(entity.id.as_str()) {
                                    if view.allows(entity.id.as_str(), true) {
                                        authorized.push(entity);
                                    }
                                } else if !entity_is_code_kind(&entity)
                                    && ordinary_entity_authorized(
                                        &policy_authorizer,
                                        &graph,
                                        &caller,
                                        &entity,
                                    )
                                    .await?
                                {
                                    authorized.push(entity);
                                }
                            }
                            return Ok(authorized);
                        }
                        let mut authorized = std::collections::HashSet::new();
                        for id in &requested {
                            if view.is_code_record(id) {
                                if !view.allows(id, true) {
                                    return Err(foreign_target(&[id.clone()]));
                                }
                                authorized.insert(id.clone());
                            } else if let Some(entity) =
                                await_non_code_entity(&query, scope, id).await?
                            {
                                if ordinary_entity_authorized(
                                    &policy_authorizer,
                                    &graph,
                                    &caller,
                                    &entity,
                                )
                                .await?
                                {
                                    authorized.insert(id.clone());
                                } else {
                                    return Err(foreign_target(&[id.clone()]));
                                }
                            } else {
                                return Err(foreign_target(&[id.clone()]));
                            }
                        }
                        Ok(entities
                            .into_iter()
                            .filter(|entity| authorized.contains(&entity.id.to_string()))
                            .collect())
                    }
                },
            )
            .await
    }

    /// The wrapped read surface (for unconstrained non-code reads).
    pub fn query(&self) -> &Arc<dyn KnowledgeQuery> {
        &self.query
    }
}

/// Eligible code target ids under a view (durable lineage of repo-selected
/// published bindings), sorted and bounded by the request budget.
fn eligible_lineage_records(view: &engram_retrieval::BoundCodeView, limit: usize) -> Vec<String> {
    let mut eligible: Vec<String> = view
        .code_records()
        .into_iter()
        .filter(|record| view.allows(record, true))
        .map(str::to_owned)
        .collect();
    eligible.sort();
    eligible.truncate(limit);
    eligible
}

/// Returns an existing non-code entity from the read surface.
/// Code-kind records without lineage are legacy code and are not ordinary reads.
async fn await_non_code_entity(
    query: &Arc<dyn KnowledgeQuery>,
    scope: &Scope,
    id: &str,
) -> CoreResult<Option<KnowledgeEntity>> {
    Ok(query
        .list_entities(scope)
        .await?
        .into_iter()
        .find(|entity| entity.id.as_str() == id && !entity_is_code_kind(entity)))
}

/// Resolves one ordinary entity's policy authority without bypassing the
/// configured policy authorizer.
async fn ordinary_entity_authorized(
    authorizer: &Option<Arc<dyn engram_runtime::PolicyAuthorizer>>,
    graph_repository: &Option<Arc<dyn engram_knowledge::KnowledgeGraphRepository>>,
    actor: &engram_domain::Actor,
    entity: &KnowledgeEntity,
) -> CoreResult<bool> {
    let Some(authorizer) = authorizer else {
        return Ok(false);
    };
    let requester = engram_domain::Requester {
        actor: actor.clone(),
        roles: Vec::new(),
        permissions: Vec::new(),
        on_behalf_of: None,
    };
    let policy = match &entity.graph_id {
        None => engram_domain::Policy::graph_lane_default(),
        Some(graph_id) => {
            let Some(graphs) = graph_repository else {
                return Ok(false);
            };
            let Some(graph) = graphs.get_graph(graph_id, &entity.scope).await? else {
                return Ok(false);
            };
            if graph.scope != entity.scope {
                return Ok(false);
            }
            graph.policy
        }
    };
    Ok(authorizer
        .can_retrieve(&requester, &entity.scope, &policy)
        .is_ok())
}

/// True for symbol / structural code-artifact entity kinds.
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

/// Typed denial for foreign/unpublished/unknown code targets.
fn foreign_target(ids: &[String]) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "BindingRequired: code target ids are foreign or unpublished in this scope: {}",
            ids.join(", ")
        ),
    }
}
