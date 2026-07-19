use chrono::{TimeZone, Utc};
use engram_domain::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn ts(hour: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 7, 2, hour, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-build"),
        kind: ActorKind::System,
        display_name: None,
        metadata: None,
    }
}

fn scope() -> Scope {
    Scope {
        tenant: "tenant-build".to_owned(),
        subject: None,
        workspace: Some("workspace-build".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "hierarchy-builder".to_owned(),
        actor: actor(),
        observed_at: ts(10),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("deterministic_fixture".to_owned()),
    }
}

#[test]
fn hierarchy_build_record_uses_contract_json_names() {
    let record = HierarchyBuildRecord {
        id: "build-1".to_owned(),
        scope: scope(),
        config: HierarchyBuildConfig {
            id: "config-1".to_owned(),
            algorithm: "deterministic-layered-fixture".to_owned(),
            version: "1".to_owned(),
            target_cluster_size: Some(4),
            max_layers: Some(3),
            similarity_metric: Some("none".to_owned()),
            inter_cluster_threshold: None,
            llm_budget: Some(0),
            created_at: ts(9),
        },
        status: HierarchyBuildStatus::CompletedWithErrors,
        input_refs: vec![EvidenceRef {
            target_type: EvidenceTargetType::Event,
            target_id: Some("event-1".to_owned()),
            uri: None,
            quote: None,
            location: None,
        }],
        output_node_ids: vec![Id::from("node-1")],
        output_relation_ids: vec!["relation-1".to_owned()],
        stats: Some(BTreeMap::from([("nodesCreated".to_owned(), json!(1))])),
        errors: vec![ConsolidationError {
            task: Some(ConsolidationTaskKind::HierarchyBuild),
            code: "partial_input".to_owned(),
            message: "one optional input was skipped".to_owned(),
            target_type: Some("event".to_owned()),
            target_id: Some("event-skipped".to_owned()),
            recoverable: true,
        }],
        provenance: provenance(),
        started_at: ts(11),
        completed_at: Some(ts(12)),
    };

    let value = serde_json::to_value(record).expect("serialize build record");

    assert_eq!(value["config"]["targetClusterSize"], json!(4));
    assert_eq!(value["status"], json!("completed_with_errors"));
    assert_eq!(value["outputNodeIds"], json!(["node-1"]));
    assert_eq!(value["stats"]["nodesCreated"], json!(1));
    assert_eq!(value["startedAt"], json!(ts(11)));
    assert!(value.get("started_at").is_none());
    assert!(matches!(value, Value::Object(_)));
}
