//! Typed canonical-port JSON adaptation for the isolated fixture consumer.
use engram_domain::*;
use engram_integration::EngramProvider;
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use futures::executor::block_on;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) struct ScopePolicy(pub(crate) Scope);
impl ScopePolicy {
    pub(crate) fn check(&self, scope: &Scope) -> CoreResult<()> {
        if &self.0 == scope {
            Ok(())
        } else {
            Err(CoreError::PolicyDenied {
                reason: "outside fixture launch scope".into(),
            })
        }
    }
}
impl PolicyAuthorizer for ScopePolicy {
    fn can_write(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.check(scope)
    }
    fn can_retrieve(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.check(scope)
    }
    fn can_forget(&self, _: &Requester, scope: &Scope, _: &Policy) -> CoreResult<()> {
        self.check(scope)
    }
}

pub(crate) fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
pub(crate) fn decode<T: DeserializeOwned>(value: &Value) -> CoreResult<T> {
    serde_json::from_value(value.clone()).map_err(|_| invalid("invalid typed operation arguments"))
}
pub(crate) fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> CoreResult<T> {
    decode(args.get(key).unwrap_or(args))
}
pub(crate) fn field<T: DeserializeOwned>(args: &Value, key: &str) -> CoreResult<T> {
    decode(args.get(key).unwrap_or(&Value::Null))
}
pub(crate) fn optional<T: DeserializeOwned>(args: &Value, key: &str) -> CoreResult<Option<T>> {
    decode(args.get(key).unwrap_or(&Value::Null))
}
pub(crate) fn text<'a>(args: &'a Value, key: &str) -> CoreResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("required operation string missing"))
}
pub(crate) fn encoded<T: Serialize>(value: T) -> CoreResult<Value> {
    serde_json::to_value(value).map_err(|_| invalid("operation response encoding failed"))
}
pub(crate) fn digest(value: &Value) -> CoreResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid("response encoding failed"))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
pub(crate) fn error_class(error: &CoreError) -> &'static str {
    match error {
        CoreError::NotFound { .. } => "NotFound",
        CoreError::PolicyDenied { .. } => "PolicyDenied",
        CoreError::InvalidRequest { .. } => "InvalidRequest",
        CoreError::Adapter { .. } => "Adapter",
        CoreError::Conflict { .. } => "Conflict",
        CoreError::ProviderUnavailable { .. } => "ProviderUnavailable",
        CoreError::MigrationPending { .. } => "MigrationPending",
        CoreError::DryRunMode { .. } => "DryRunMode",
        CoreError::CapabilityUnsupported { .. } => "CapabilityUnsupported",
        CoreError::EmbeddingSpaceMismatch { .. } => "EmbeddingSpaceMismatch",
        CoreError::MigrationManifestStale { .. } => "MigrationManifestStale",
        CoreError::MigrationFailed { .. } => "MigrationFailed",
        CoreError::TransactionUnsupported { .. } => "TransactionUnsupported",
        CoreError::ValidationFailed { .. } => "ValidationFailed",
        CoreError::BackendTransient { .. } => "BackendTransient",
        CoreError::BackendPermanent { .. } => "BackendPermanent",
    }
}
pub(crate) fn scope_guard(value: &Value, scope: &Scope) -> CoreResult<()> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "scope" {
                    let actual: Scope = if let Some(raw) = child.as_str() {
                        serde_json::from_str(raw).map_err(|_| invalid("invalid archive scope"))?
                    } else {
                        decode(child)?
                    };
                    ScopePolicy(scope.clone()).check(&actual)?;
                } else {
                    scope_guard(child, scope)?;
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                scope_guard(child, scope)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn references(value: &Value, results: &BTreeMap<String, Value>) -> CoreResult<Value> {
    match value {
        Value::Object(map) if map.len() == 1 && map.contains_key("$result") => {
            let reference = map["$result"]
                .as_str()
                .ok_or_else(|| invalid("invalid result reference"))?;
            let (id, pointer) = reference
                .split_once('#')
                .ok_or_else(|| invalid("invalid result reference"))?;
            results
                .get(id)
                .and_then(|v| v.pointer(pointer))
                .cloned()
                .ok_or_else(|| invalid("missing prior call result reference"))
        }
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| Ok((k.clone(), references(v, results)?)))
            .collect::<CoreResult<serde_json::Map<String, Value>>>()
            .map(Value::Object),
        Value::Array(values) => values
            .iter()
            .map(|v| references(v, results))
            .collect::<CoreResult<Vec<_>>>()
            .map(Value::Array),
        _ => Ok(value.clone()),
    }
}
pub(crate) fn actor(args: &Value) -> CoreResult<Actor> {
    field(args, "actor")
}
pub(crate) fn window(args: &Value) -> CoreResult<engram_integration::TimeWindow> {
    let w = args.get("window").unwrap_or(args);
    if !w.is_object() {
        return Err(invalid("invalid observed-at window"));
    }
    Ok(engram_integration::TimeWindow {
        from: optional(w, "from")?,
        to: optional(w, "to")?,
    })
}
pub(crate) fn unavailable(capability: &str) -> CoreError {
    CoreError::CapabilityUnsupported {
        capability: capability.into(),
        reason: "selected provider port is not wired".into(),
    }
}

pub(crate) fn source_parent(p: &EngramProvider, scope: &Scope, id: &SourceId) -> CoreResult<()> {
    let parent = block_on(p.require_knowledge()?.get_source(id, scope))?
        .ok_or_else(|| invalid("source parent not in fixture scope"))?;
    ScopePolicy(scope.clone()).check(&parent.scope)
}
pub(crate) fn ontology_parent(
    p: &EngramProvider,
    scope: &Scope,
    id: &OntologyId,
) -> CoreResult<()> {
    let parent = block_on(p.require_ontology()?.get_ontology(id, scope))?
        .ok_or_else(|| invalid("ontology parent not in fixture scope"))?;
    ScopePolicy(scope.clone()).check(&parent.scope)
}
pub(crate) fn scheme_parent(
    p: &EngramProvider,
    scope: &Scope,
    id: &ConceptSchemeId,
) -> CoreResult<()> {
    let parent = block_on(p.require_taxonomy()?.get_concept_scheme(id, scope))?
        .ok_or_else(|| invalid("scheme parent not in fixture scope"))?;
    ScopePolicy(scope.clone()).check(&parent.scope)
}
pub(crate) fn provenance_rows(rows: Vec<engram_integration::ProvenanceEntry>) -> Value {
    Value::Array(
        rows.into_iter()
            .map(|r| json!({"target":r.target,"targetId":r.target_id,"provenance":r.provenance}))
            .collect(),
    )
}
pub(crate) fn batch_value(outcome: engram_integration::BatchOutcome) -> Value {
    let steps: Vec<_> = outcome
        .steps
        .into_iter()
        .map(|step| {
            json!({
                "step":step.step,"status":step.status,"supersession":step.supersession,
                "error":step.error.as_ref().map(|e|e.to_redacted().to_string()),
                "errorClass":step.error.as_ref().map(error_class),
            })
        })
        .collect();
    json!({"guarantee":outcome.guarantee,"status":outcome.status,"steps":steps})
}
pub(crate) fn dispatch(
    p: &EngramProvider,
    scope: &Scope,
    operation: &str,
    a: &Value,
) -> CoreResult<Value> {
    if !a.is_object() {
        return Err(invalid("operation arguments must be an object"));
    }
    scope_guard(a, scope)?;
    macro_rules! run {
        ($future:expr) => {
            encoded(block_on($future)?)
        };
    }
    match operation {
        "memory.write" => run!(p.require_memory()?.write_memory(arg(a, "request")?)),
        "memory.retrieve" => run!(p.require_memory()?.retrieve(arg(a, "request")?)),
        "memory.forget" => run!(p.require_memory()?.forget(arg(a, "request")?)),
        "memory.events" => run!(p.require_memory()?.list_events_for_scope(scope)),
        "batch.ingest" => Ok(batch_value(block_on(
            p.require_batch()?.ingest(arg(a, "request")?),
        )?)),
        "knowledge.put_source" => run!(p.require_knowledge()?.put_source(arg(a, "source")?)),
        "knowledge.put_document" => {
            let record: SourceDocument = arg(a, "document")?;
            source_parent(p, scope, &record.source_id)?;
            run!(p.require_knowledge()?.put_document(record))
        }
        "knowledge.put_chunk" => {
            let record: KnowledgeChunk = arg(a, "chunk")?;
            source_parent(p, scope, &record.source_id)?;
            run!(p.require_knowledge()?.put_chunk(record))
        }
        "knowledge.put_entity" => run!(p.require_knowledge()?.put_entity(arg(a, "entity")?)),
        "knowledge.put_relationship" => run!(
            p.require_knowledge()?
                .put_relationship(arg(a, "relationship")?)
        ),
        "knowledge.get_entity" => run!(
            p.require_knowledge()?
                .get_entity(&EntityId::from(text(a, "id")?), scope)
        ),
        "knowledge.get_source" => run!(
            p.require_knowledge()?
                .get_source(&SourceId::from(text(a, "id")?), scope)
        ),
        "knowledge.list_entities" => run!(p.require_knowledge_query()?.list_entities(scope)),
        "graph.put" => run!(p.require_graph()?.put_graph(arg(a, "graph")?)),
        "graph.neighbors" => run!(p.require_graph()?.neighbors(
            &KnowledgeGraphId::from(text(a, "graphId")?),
            &EntityId::from(text(a, "nodeId")?),
            scope,
            optional(a, "limit")?
        )),
        "identity.resolve_entity" => run!(
            p.identity()
                .ok_or_else(|| unavailable("identity"))?
                .resolve_or_put_entity(arg(a, "request")?)
        ),
        "identity.resolve_relationship" => run!(
            p.identity()
                .ok_or_else(|| unavailable("identity"))?
                .resolve_or_put_relationship(arg(a, "relationship")?)
        ),
        "ontology.put" => run!(p.require_ontology()?.put_ontology(arg(a, "ontology")?)),
        "ontology.put_class" => {
            let r: OntologyClass = arg(a, "class")?;
            ontology_parent(p, scope, &r.ontology_id)?;
            run!(p.require_ontology()?.put_class(r))
        }
        "ontology.put_property" => {
            let r: OntologyProperty = arg(a, "property")?;
            ontology_parent(p, scope, &r.ontology_id)?;
            run!(p.require_ontology()?.put_property(r))
        }
        "ontology.put_axiom" => {
            let r: OntologyAxiom = arg(a, "axiom")?;
            ontology_parent(p, scope, &r.ontology_id)?;
            run!(p.require_ontology()?.put_axiom(r))
        }
        "ontology.get" => run!(
            p.require_ontology()?
                .get_ontology(&OntologyId::from(text(a, "id")?), scope)
        ),
        "ontology.validate" => run!(p.require_ontology()?.validate_graph(
            &KnowledgeGraphId::from(text(a, "graphId")?),
            &OntologyId::from(text(a, "ontologyId")?),
            scope
        )),
        "taxonomy.put_scheme" => run!(p.require_taxonomy()?.put_concept_scheme(arg(a, "scheme")?)),
        "taxonomy.get_scheme" => run!(
            p.require_taxonomy()?
                .get_concept_scheme(&ConceptSchemeId::from(text(a, "id")?), scope)
        ),
        "taxonomy.put_concept" => {
            let r: Concept = arg(a, "concept")?;
            scheme_parent(p, scope, &r.scheme_id)?;
            run!(p.require_taxonomy()?.put_concept(r))
        }
        "taxonomy.put_relation" => {
            let r: ConceptRelation = arg(a, "relation")?;
            scheme_parent(p, scope, &r.scheme_id)?;
            run!(p.require_taxonomy()?.put_concept_relation(r))
        }
        "taxonomy.list" => {
            let id = ConceptSchemeId::from(text(a, "schemeId")?);
            scheme_parent(p, scope, &id)?;
            run!(p.require_taxonomy()?.list_concepts(&id, scope))
        }
        "beliefs.put" => run!(p.require_beliefs()?.put_belief(arg(a, "belief")?)),
        "beliefs.get" => {
            let query = engram_belief::BeliefQuery {
                scope: scope.clone(),
                subject_key: optional(a, "subjectKey")?,
                valid_at: optional(a, "validAt")?,
                recorded_at: optional(a, "recordedAt")?,
                statuses: optional(a, "statuses")?.unwrap_or_else(|| vec![BeliefStatus::Active]),
                include_stale: optional(a, "includeStale")?.unwrap_or(false),
                order: engram_belief::BeliefQueryOrder::LatestValidFirst,
            };
            run!(p.require_beliefs()?.get_belief(query))
        }
        "beliefs.retract" => run!(p.require_beliefs()?.retract_belief(
            &BeliefId::from(text(a, "id")?),
            scope,
            field(a, "at")?
        )),
        "beliefs.stale" => run!(p.require_beliefs()?.list_stale(scope)),
        "beliefs.contradictions" => run!(p.require_beliefs()?.list_contradictions(scope)),
        "beliefs.put_contradiction" => run!(
            p.require_beliefs()?
                .put_contradiction(arg(a, "contradiction")?)
        ),
        "hierarchy.put_node" => run!(p.require_hierarchy()?.put_node(arg(a, "node")?)),
        "hierarchy.put_relation" => run!(p.require_hierarchy()?.put_relation(arg(a, "relation")?)),
        "hierarchy.path" => run!(p.require_hierarchy()?.path_for(
            &field::<Vec<String>>(a, "seedIds")?,
            scope,
            optional(a, "maxLayer")?
        )),
        "retrieval.recall" => run!(p.require_recall()?.recall(arg(a, "request")?)),
        "procedures.put" => run!(
            p.require_procedures()?
                .upsert_procedure(arg(a, "procedure")?)
        ),
        "procedures.list" => run!(p.require_procedures()?.list_procedures(scope)),
        "procedures.increment_success" => run!(
            p.require_procedures()?
                .increment_success(&ProcedureId::from(text(a, "id")?), scope)
        ),
        "proposals.submit" => run!(p.require_proposals()?.submit(arg(a, "request")?)),
        "proposals.list" => run!(p.require_proposals()?.list(arg(a, "request")?)),
        "proposals.approve" => run!(p.require_proposals()?.approve(arg(a, "request")?)),
        "proposals.reject" => run!(p.require_proposals()?.reject(arg(a, "request")?)),
        "outcomes.report" => run!(p.require_outcomes()?.report_outcome(arg(a, "request")?)),
        "outcomes.list" => run!(p.require_outcomes()?.list_outcomes(&arg(a, "window")?)),
        "provenance.for" => run!(p.require_provenance()?.provenance_for(
            field(a, "target")?,
            text(a, "id")?,
            scope
        )),
        "provenance.evidence" => run!(p.require_provenance()?.evidence_for(
            field(a, "target")?,
            text(a, "id")?,
            scope
        )),
        "provenance.by_source" => Ok(provenance_rows(block_on(
            p.require_provenance()?
                .provenance_by_source(text(a, "source")?, scope, window(a)?),
        )?)),
        "provenance.by_scope" => Ok(provenance_rows(block_on(
            p.require_provenance()?.evidence_by_scope(
                scope,
                window(a)?,
                optional(a, "limit")?.unwrap_or(100),
            ),
        )?)),
        "provenance.attach" => run!(p.require_provenance()?.attach_evidence(
            field(a, "target")?,
            text(a, "id")?,
            field(a, "evidence")?,
            scope
        )),
        "consolidation.run" => run!(p.require_consolidation()?.consolidate(arg(a, "request")?)),
        "observability.diagnostics" => run!(p.require_observability()?.diagnostics()),
        "community.overview" => run!(
            p.community_query()
                .ok_or_else(|| unavailable("community"))?
                .overview(scope, optional(a, "limit")?.unwrap_or(20))
        ),
        "community.member_index" => run!(
            p.community_query()
                .ok_or_else(|| unavailable("community"))?
                .member_index(scope)
        ),
        "community.of" => run!(
            p.community_query()
                .ok_or_else(|| unavailable("community"))?
                .community_of(scope, text(a, "id")?)
        ),
        "community.scope_counts" => run!(
            p.community_query()
                .ok_or_else(|| unavailable("community"))?
                .scope_counts(scope)
        ),
        "scope.inventory" => run!(p.require_scope_inventory()?.inventory()),
        "archive.export" => run!(p.require_export_import()?.export(scope)),
        "archive.dry_run" => encoded(p.require_migration()?.dry_run_import(&arg(a, "data")?)?),
        "archive.apply" => {
            p.require_migration()?.apply_import(&arg(a, "manifest")?)?;
            Err(invalid(
                "canonical archive apply unexpectedly returned success",
            ))
        }
        "maintenance.plan" => run!(
            p.require_graph_maintenance()?
                .build_plan(arg(a, "request")?)
        ),
        "maintenance.apply" => run!(
            p.require_graph_maintenance()?
                .apply_plan(&field(a, "plan")?, field(a, "mode")?)
        ),
        "maintenance.candidates" => run!(p.require_graph_maintenance()?.detect_candidates(
            scope,
            optional::<KnowledgeGraphId>(a, "graphId")?.as_ref(),
            &field(a, "policy")?
        )),
        "maintenance.health" => run!(
            p.require_graph_maintenance()?
                .graph_health(scope, optional::<KnowledgeGraphId>(a, "graphId")?.as_ref())
        ),
        "code.register" => run!(p.code_bindings()?.register(
            &actor(a)?,
            scope,
            arg::<engram_knowledge::RepositoryRegistration>(a, "registration")?
        )),
        "code.bind" => run!(p.code_bindings()?.bind(
            &actor(a)?,
            scope,
            &RepoId(text(a, "repo")?.into()),
            text(a, "branchRef")?
        )),
        "code.status" => run!(p.code_bindings()?.status(
            &actor(a)?,
            scope,
            &RepoId(text(a, "repo")?.into()),
            a.get("attemptId").and_then(Value::as_str)
        )),
        "code.refresh" => run!(p.code_bindings()?.refresh_from_registration(
            &actor(a)?,
            scope,
            &RepoId(text(a, "repo")?.into())
        )),
        "code.scan" => run!(p.code_bindings()?.ingest_registered(
            &actor(a)?,
            scope,
            &RepoId(text(a, "repo")?.into()),
            a.get("branchRef").and_then(Value::as_str),
            None,
            p.code_ingestion()?
        )),
        "code.source" => {
            let view = block_on(p.code_fetch_exact(
                &actor(a)?,
                scope,
                text(a, "repo")?,
                text(a, "path")?,
                None,
            ))?;
            Ok(
                json!({"bytes":view.bytes,"fullContentDigest":view.full_content_digest,"servedDigest":view.served_digest,"commitOid":view.commit_oid,"path":view.path,"span":view.span}),
            )
        }
        "code.reset_preview" => run!(p.code_bindings()?.preview_reset(
            &actor(a)?,
            scope,
            &RepoId(text(a, "repo")?.into())
        )),
        "code.reset_apply" => {
            let coordinator = p.code_bindings()?;
            let journal = block_on(coordinator.journal_reset(
                &actor(a)?,
                field(a, "journal")?,
                field(a, "decision")?,
            ))?;
            run!(coordinator.apply_reset(&actor(a)?, journal))
        }
        "code.reset_resume" => run!(
            p.code_bindings()?
                .apply_reset(&actor(a)?, field(a, "journal")?)
        ),
        _ => Err(invalid("missing actual operation dispatch")),
    }
}

pub(crate) fn supported(operation: &str) -> bool {
    matches!(
        operation,
        "memory.write"
            | "memory.retrieve"
            | "memory.forget"
            | "batch.ingest"
            | "knowledge.put_source"
            | "knowledge.put_document"
            | "knowledge.put_chunk"
            | "knowledge.put_entity"
            | "knowledge.put_relationship"
            | "knowledge.get_entity"
            | "graph.neighbors"
            | "identity.resolve_entity"
            | "identity.resolve_relationship"
            | "ontology.put"
            | "ontology.put_class"
            | "ontology.put_property"
            | "ontology.put_axiom"
            | "ontology.get"
            | "ontology.validate"
            | "taxonomy.put_scheme"
            | "taxonomy.get_scheme"
            | "taxonomy.put_concept"
            | "taxonomy.put_relation"
            | "taxonomy.list"
            | "beliefs.put"
            | "beliefs.get"
            | "beliefs.retract"
            | "beliefs.stale"
            | "beliefs.contradictions"
            | "hierarchy.path"
            | "retrieval.recall"
            | "procedures.put"
            | "procedures.list"
            | "procedures.increment_success"
            | "proposals.submit"
            | "proposals.list"
            | "proposals.approve"
            | "proposals.reject"
            | "outcomes.report"
            | "outcomes.list"
            | "provenance.for"
            | "provenance.evidence"
            | "provenance.by_source"
            | "provenance.by_scope"
            | "provenance.attach"
            | "consolidation.run"
            | "observability.diagnostics"
            | "community.overview"
            | "community.member_index"
            | "community.of"
            | "community.scope_counts"
            | "scope.inventory"
            | "archive.export"
            | "archive.dry_run"
            | "archive.apply"
            | "maintenance.plan"
            | "maintenance.apply"
            | "maintenance.candidates"
            | "maintenance.health"
            | "code.register"
            | "code.bind"
            | "code.status"
            | "code.refresh"
            | "code.scan"
            | "code.source"
            | "code.reset_preview"
            | "code.reset_apply"
            | "code.reset_resume"
            | "memory.events"
            | "knowledge.get_source"
            | "knowledge.list_entities"
            | "graph.put"
            | "beliefs.put_contradiction"
            | "hierarchy.put_node"
            | "hierarchy.put_relation"
    )
}
