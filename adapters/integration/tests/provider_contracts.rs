//! Pure catalogue contracts and mutation controls; no provider/engine is opened.
//! These prove inventory consistency, never native workflow or fault execution.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

// Independent host-route contract, grounded in canonical ports and current host
// entry points. Updating fixture JSON alone cannot bless a missing/wrong route.
const ROUTES: &str = "
memory.write writeJson write_memory
memory.retrieve searchJson recall
memory.forget forgetJson forget
batch.ingest ingestJson store_knowledge
knowledge.put_source ingestJson store_knowledge
knowledge.put_document ingestJson store_knowledge
knowledge.put_chunk ingestJson store_knowledge
knowledge.put_entity putEntityJson put_entity
knowledge.put_relationship putRelationshipJson put_relationship
knowledge.get_entity getEntityJson resolve_entity
graph.neighbors neighborsJson graph_neighbors
identity.resolve_entity resolveOrPutEntityJson identity_resolve_or_put_entity
identity.resolve_relationship resolveOrPutRelationshipJson identity_resolve_or_put_relationship
ontology.put putOntologyJson ontology_put
ontology.put_class putClassJson ontology_put_class
ontology.put_property putPropertyJson ontology_put_property
ontology.put_axiom putAxiomJson ontology_put_axiom
ontology.get getOntologyJson ontology_get
ontology.validate validateGraphJson ontology_validate_graph
taxonomy.put_scheme putConceptSchemeJson taxonomy_put_scheme
taxonomy.get_scheme getConceptSchemeJson taxonomy_get_scheme
taxonomy.put_concept putConceptJson taxonomy_put_concept
taxonomy.put_relation putConceptRelationJson taxonomy_put_relation
taxonomy.list listConceptsJson taxonomy_list_concepts
beliefs.put upsertBeliefJson belief_put
beliefs.get getBeliefJson belief_get
beliefs.retract retractBeliefJson belief_retract
beliefs.stale listStaleBeliefsJson belief_stale_list
beliefs.contradictions listContradictionsJson contradiction_list
hierarchy.path pathForJson hierarchy_path
retrieval.recall recallJson recall
procedures.put upsertJson procedure_put
procedures.list listJson procedure_list
procedures.increment_success incrementSuccessJson procedure_increment
proposals.submit submitJson proposal_submit
proposals.list listJson proposal_list
proposals.approve approveJson proposal_approve
proposals.reject rejectJson proposal_reject
outcomes.report reportOutcomeJson report_outcome
outcomes.list listOutcomesJson list_outcomes
provenance.for provenanceForJson provenance_for
provenance.evidence evidenceForJson evidence_for
provenance.by_source provenanceBySourceJson provenance_by_source
provenance.by_scope evidenceByScopeJson evidence_by_scope
provenance.attach attachEvidenceJson attach_evidence
consolidation.run consolidateJson consolidate
observability.diagnostics diagnosticsJson provider_diagnostics
community.overview overviewJson community_overview
community.member_index memberIndexJson community_member_index
community.of communityOfJson community_of
community.scope_counts scopeCountsJson community_scope_counts
scope.inventory inventoryJson scope_inventory
archive.export exportJson archive_export
archive.dry_run dryRunImportJson archive_dry_run
archive.apply applyImportJson archive_apply
maintenance.plan buildPlanJson build_maintenance_plan
maintenance.apply applyPlanJson apply_maintenance_plan
maintenance.candidates listMaintenanceCandidatesJson list_maintenance_candidates
maintenance.health graphHealthJson graph_health
code.register codeRegisterJson code_repo_register
code.bind codeBindJson code_bind
code.status codeStatusJson code_status
code.refresh codeRefreshJson code_refresh
code.scan scanRepositoryJson scan_repo
code.source codeFetchJson fetch_content
code.reset_preview codeResetPreviewJson code_reset_preview
code.reset_apply codeResetApplyJson code_reset_apply
code.reset_resume codeResetResumeJson code_reset_resume
code.backfill backfillEmbeddingsJson reindex
";
fn capability(operation: &str) -> &'static str {
    match operation.split('.').next().unwrap() {
        "memory" => "memory",
        "batch" => "atomic_batch",
        "knowledge" => "knowledge",
        "graph" => "graph",
        "identity" => "entity_identity",
        "ontology" => "ontology",
        "taxonomy" => "taxonomy",
        "beliefs" => "beliefs",
        "hierarchy" => "hierarchy",
        "retrieval" => "unified_recall",
        "procedures" => "procedures",
        "proposals" => "proposals",
        "outcomes" => "outcomes",
        "provenance" => "episodes_evidence",
        "consolidation" => "consolidation",
        "observability" => "observability",
        "community" => "community_query",
        "scope" => "scope_inventory",
        "archive" if operation == "archive.export" => "export_import",
        "archive" => "migration",
        "maintenance" => "graph_maintenance",
        "code" => "code_binding",
        _ => panic!("unknown independent contract family"),
    }
}
fn getter(operation: &str) -> Option<&'static str> {
    match capability(operation) {
        "memory" => Some("requireMemoryApi"),
        "atomic_batch" => Some("requireBatchApi"),
        "knowledge"
            if matches!(
                operation,
                "knowledge.put_source" | "knowledge.put_document" | "knowledge.put_chunk"
            ) =>
        {
            Some("requireBatchApi")
        }
        "knowledge" | "graph" | "entity_identity" => Some("requireGraphApi"),
        "ontology" => Some("requireOntologyApi"),
        "taxonomy" => Some("requireTaxonomyApi"),
        "beliefs" => Some("requireBeliefsApi"),
        "hierarchy" => Some("requireHierarchyApi"),
        "unified_recall" => Some("requireRecallApi"),
        "procedures" => Some("requireProceduresApi"),
        "proposals" => Some("requireProposalsApi"),
        "outcomes" => Some("requireOutcomesApi"),
        "episodes_evidence" => Some("requireProvenanceApi"),
        "observability" => Some("requireObservabilityApi"),
        "community_query" => Some("requireCommunityQueryApi"),
        "scope_inventory" => Some("requireScopeInventoryApi"),
        "export_import" => Some("requireExportImportApi"),
        "migration" => Some("requireMigrationApi"),
        "graph_maintenance" => Some("requireGraphMaintenanceApi"),
        "code_binding" | "consolidation" => None,
        _ => panic!("unknown independent native owner"),
    }
}
fn oneshot(operation: &str) -> Option<&'static str> {
    match operation {
        "ontology.put"
        | "ontology.put_class"
        | "ontology.put_property"
        | "taxonomy.put_scheme"
        | "taxonomy.put_concept"
        | "taxonomy.put_relation" => Some("pack"),
        "retrieval.recall" => Some("recall"),
        "code.scan" => Some("code"),
        "code.source" => Some("fetch-content"),
        "code.backfill" => Some("backfill-embeddings"),
        op if op.starts_with("code.") => Some("code"),
        _ => None,
    }
}
fn rows() -> Vec<Value> {
    let provider: Value =
        serde_json::from_str(include_str!("fixtures/provider_operations.json")).unwrap();
    let code: Value =
        serde_json::from_str(include_str!("fixtures/provider_code_operations.json")).unwrap();
    provider["operations"]
        .as_array()
        .unwrap()
        .iter()
        .chain(code["operations"].as_array().unwrap())
        .cloned()
        .collect()
}
fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing {key}"))
}
fn validate(rows: &[Value]) -> Result<(), String> {
    let expected: BTreeMap<_, _> = ROUTES
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields[0], (fields[1], fields[2]))
        })
        .collect();
    let mut seen = BTreeSet::new();
    for row in rows {
        let operation = required_string(row, "operation")?;
        if !seen.insert(operation) {
            return Err("duplicate operation".into());
        }
        let (method, mcp) = expected
            .get(operation)
            .ok_or("unknown required operation")?;
        if row["capability"] != capability(operation) {
            return Err("wrong capability owner".into());
        }
        if row["native"].get("getter").is_none() {
            return Err("native owner must be explicitly declared".into());
        }
        if row["native"]["method"] != *method
            || !method.ends_with("Json")
            || row["native"]["applicable"] == false
        {
            return Err("missing/wrong native business method; getter is not execution".into());
        }
        if row["native"]["getter"].as_str() != getter(operation) || row["mcp_tool"] != *mcp {
            return Err("wrong host route".into());
        }
        if row["expected_error_reason_required"] != true {
            return Err("error reason requirement omitted".into());
        }
        let assertions = row["assertions_required"]
            .as_array()
            .ok_or("assertion inventory missing")?;
        for key in ["selected_provider", "full_scope", "expected_outcome"] {
            if !assertions.contains(&Value::String(key.into())) {
                return Err("required assertion omitted".into());
            }
        }
        if operation.starts_with("code.")
            && !assertions.contains(&Value::String("code_authority".into()))
        {
            return Err("code authority assertion omitted".into());
        }
        let declaration = &row["oneshot"];
        let applicable = declaration["applicable"]
            .as_bool()
            .ok_or("one-shot applicability must be explicit")?;
        if applicable != oneshot(operation).is_some() {
            return Err("wrong one-shot applicability".into());
        }
        if required_string(declaration, "reason")?.len() < 20 {
            return Err("one-shot reason is not explicit".into());
        }
        if let Some(command) = oneshot(operation) {
            if declaration["command"] != command {
                return Err("wrong actual one-shot command".into());
            }
            let expected_op = match operation {
                "code.scan" => Some("rebuild"),
                "code.source" => Some("fetch-content"),
                _ => None,
            };
            if let Some(expected) = expected_op {
                if declaration["operation"] != expected {
                    return Err("wrong actual one-shot operation".into());
                }
            }
        } else if declaration.get("command").is_some() {
            return Err("N/A cannot claim an executed one-shot command".into());
        }
        if operation == "code.backfill" {
            if row["rust"]["applicable"] != false {
                return Err("Rust backfill must remain explicit source-backed N/A".into());
            }
            let reason = required_string(&row["rust"], "reason")?;
            if !reason.contains("EngramProvider") || !reason.contains("MCP") {
                return Err("missing actual Rust N/A owner explanation".into());
            }
            let sources = row["rust"]["source"]
                .as_array()
                .ok_or("source anchors missing")?;
            for path in [
                "core/integration/src/provider.rs",
                "mcp/engram-mcp/src/codegraph.rs",
                "bindings/node/src/provider.rs",
            ] {
                if !sources.iter().any(|s| {
                    s.as_str()
                        .is_some_and(|s| s.split(':').next() == Some(path))
                }) {
                    return Err("Rust N/A source anchor missing".into());
                }
            }
            if !required_string(row, "native_expected")?.contains("CapabilityUnsupported") {
                return Err(
                    "native backfill is applicable and must expose its actual unsupported reason"
                        .into(),
                );
            }
        } else if row.get("rust").is_some_and(|r| r["applicable"] == false) {
            return Err("unapproved Rust host omission".into());
        }
    }
    if seen != expected.keys().copied().collect::<BTreeSet<_>>() {
        return Err("required operation omitted".into());
    }
    Ok(())
}
#[test]
fn current_catalogue_has_complete_explicit_operation_host_contracts() {
    let all = rows();
    assert_eq!(all.len(), 69);
    validate(&all).unwrap();
}
#[test]
fn omissions_wrong_capabilities_routes_and_reasons_are_rejected() {
    let all = rows();
    for index in 0..all.len() {
        let mut missing = all.clone();
        missing.remove(index);
        assert!(validate(&missing).is_err());
        let mut wrong = all.clone();
        wrong[index]["capability"] = Value::String("wrong-owner".into());
        assert!(validate(&wrong).is_err());
        let mut wrong = all.clone();
        wrong[index]["native"]["method"] = Value::String("requireMemoryApi".into());
        assert!(validate(&wrong).is_err());
        let mut wrong = all.clone();
        wrong[index]["expected_error_reason_required"] = Value::Bool(false);
        assert!(validate(&wrong).is_err());
        let mut wrong = all.clone();
        wrong[index]["oneshot"]
            .as_object_mut()
            .unwrap()
            .remove("reason");
        assert!(validate(&wrong).is_err());
    }
    let mut wrong = all.clone();
    wrong[0]["native"]["method"] = Value::String("searchJson".into());
    assert!(validate(&wrong).is_err());
    for (operation, bad_op) in [("code.scan", "scan"), ("code.source", "fetch")] {
        let mut wrong = all.clone();
        let row = wrong
            .iter_mut()
            .find(|r| r["operation"] == operation)
            .unwrap();
        row["oneshot"]["operation"] = Value::String(bad_op.into());
        assert!(validate(&wrong).is_err());
    }
    let mut wrong = all.clone();
    wrong[0]["rust"] = serde_json::json!({"applicable":false,"reason":"not built"});
    assert!(validate(&wrong).is_err());
    let mut wrong = all.clone();
    let row = wrong
        .iter_mut()
        .find(|r| r["operation"] == "ontology.put")
        .unwrap();
    row["mcp_tool"] = Value::String("ontology_read".into());
    assert!(validate(&wrong).is_err());
    let mut wrong = all.clone();
    wrong[0]["oneshot"] = serde_json::json!({"applicable":true,"command":"recall","reason":"invented correspondence is not an actual host contract"});
    assert!(validate(&wrong).is_err());
}
#[test]
fn source_backed_backfill_exception_cannot_hide_native_or_other_rust_cells() {
    let mut all = rows();
    let index = all
        .iter()
        .position(|r| r["operation"] == "code.backfill")
        .unwrap();
    let mut wrong = all.clone();
    wrong[index]["native"]["applicable"] = Value::Bool(false);
    assert!(validate(&wrong).is_err());
    let mut wrong = all.clone();
    wrong[index]
        .as_object_mut()
        .unwrap()
        .remove("native_expected");
    assert!(validate(&wrong).is_err());
    all[index]["rust"]["source"] = serde_json::json!([]);
    assert!(validate(&all).is_err());
}
#[test]
fn declared_routes_reference_actual_host_sources_without_claiming_execution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let native = std::fs::read_to_string(root.join("bindings/node/src/provider.rs")).unwrap();
    let main = std::fs::read_to_string(root.join("mcp/engram-mcp/src/main.rs")).unwrap();
    let tools = std::fs::read_to_string(root.join("mcp/engram-mcp/src/tools.rs")).unwrap();
    let mut declared = BTreeSet::new();
    for line in native.lines() {
        if let Some((_, rest)) = line.split_once("js_name = \"") {
            if let Some((name, _)) = rest.split_once('"') {
                declared.insert(name.to_owned());
            }
        }
        let function = line
            .split_once("pub fn ")
            .or_else(|| line.split_once("pub async fn "));
        if let Some((_, rest)) = function {
            let name = rest.split('(').next().unwrap();
            let mut parts = name.split('_');
            let mut camel = parts.next().unwrap().to_owned();
            for part in parts {
                let mut chars = part.chars();
                if let Some(first) = chars.next() {
                    camel.extend(first.to_uppercase());
                    camel.push_str(chars.as_str());
                }
            }
            declared.insert(camel);
        }
    }
    for row in rows() {
        let method = row["native"]["method"].as_str().unwrap();
        assert!(
            declared.contains(method),
            "missing native business method {method}"
        );
        if let Some(getter) = row["native"]["getter"].as_str() {
            assert!(declared.contains(getter), "missing native owner {getter}");
        }
        let name = row["mcp_tool"].as_str().unwrap();
        let quoted = format!("\"{name}\"");
        assert!(
            main.contains(&quoted) || tools.contains(&quoted),
            "missing declared MCP route {name}"
        );
    }
    assert!(native.contains("EmbeddingBackfillUnsupported"));
    assert!(main.contains("register_provider_tools(registry)"));
}

// Per-assertion applicability is narrower than host or operation applicability.
// This is an auditor-result shape control, not actual fault execution evidence.
fn validate_blocked_probe_assertion(
    host: &str,
    command: &str,
    audit: &Value,
) -> Result<(), String> {
    if audit["status"] == "N/A"
        || audit["logical_outcome"] != "UNKNOWN_RETAINED"
        || audit["landed_evidence"] != "CANONICAL_RESPONSE_CORRELATION"
        || audit["original_operation_id"]
            .as_str()
            .is_none_or(|id| id.is_empty())
    {
        return Err("whole fault case cannot be skipped or replaced by counters".into());
    }
    let assertion = &audit["blocked_admission"];
    if host == "mcp-oneshot" && matches!(command, "pack" | "code") {
        if assertion["status"] != "N/A"
            || !required_string(assertion, "reason")?.contains("same-process")
        {
            return Err("one-shot needs its exact per-assertion reason".into());
        }
        let sources = assertion["source"]
            .as_array()
            .ok_or("one-shot assertion source missing")?;
        for source in ["mcp/engram-mcp/src/main.rs", "mcp/engram-mcp/src/pack.rs"] {
            if !sources.contains(&Value::String(source.into())) {
                return Err("one-shot assertion source missing".into());
            }
        }
    } else if assertion["status"] != "PASS" {
        return Err("interactive hosts require actual exact-ID blocked-admission proof".into());
    }
    Ok(())
}
#[test]
fn only_pack_code_oneshot_post_unknown_probe_has_source_backed_assertion_na() {
    let mut audit = serde_json::json!({"logical_outcome":"UNKNOWN_RETAINED",
        "original_operation_id":"00000000-0000-4000-8000-000000000001",
        "landed_evidence":"CANONICAL_RESPONSE_CORRELATION",
        "blocked_admission":{"status":"N/A",
            "reason":"Existing pack/code one-shot command aborts on Unknown and has no same-process post-error API; a new process would reopen/replay the unresolved namespace",
            "source":["mcp/engram-mcp/src/main.rs","mcp/engram-mcp/src/pack.rs"]}});
    for command in ["pack", "code"] {
        assert!(validate_blocked_probe_assertion("mcp-oneshot", command, &audit).is_ok());
    }
    for host in ["rust", "native", "mcp-stdio"] {
        assert!(validate_blocked_probe_assertion(host, "code", &audit).is_err());
    }
    assert!(validate_blocked_probe_assertion("mcp-oneshot", "recall", &audit).is_err());
    audit["status"] = Value::String("N/A".into());
    assert!(validate_blocked_probe_assertion("mcp-oneshot", "pack", &audit).is_err());
}
