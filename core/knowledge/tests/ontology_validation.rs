use engram_domain::*;
use engram_knowledge::ontology_validation::{VALIDATE_RELATIONSHIP_LIMIT, validate_visible_graph};
use serde_json::{Value, json};
fn record<T: serde::de::DeserializeOwned>(mut value: Value) -> T {
    value["createdAt"] = "2026-11-17T00:00:00Z".into();
    value["provenance"] = json!({"source":"fixture","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"});
    serde_json::from_value(value).unwrap()
}
#[test]
fn advisory_findings_preserve_case_ids_messages_provenance_and_truncation() {
    let property: OntologyProperty = record(
        json!({"id":"property","ontologyId":"ontology","uri":"urn:REL","label":"RELates","kind":"object","status":"active"}),
    );
    let relation = |id: &str, predicate: &str| {
        record::<KnowledgeRelationship>(
            json!({"id":id,"predicate":predicate,"subject":{"id":"left"},"object":{"id":"right"},"scope":{"tenant":"tenant"}}),
        )
    };
    let now: Timestamp = "2026-11-17T00:00:00Z".parse().unwrap();
    let ontology: OntologyId = "ontology".into();
    let rows = vec![
        relation("known", "rElAtEs"),
        relation("known-uri", "URN:rel"),
        relation("z", "missing"),
    ];
    let findings = validate_visible_graph(&ontology, &[property], &rows, true, now);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].id, "finding-ontology-truncated");
    assert_eq!(findings[0].severity, OntologyValidationSeverity::Info);
    assert_eq!(
        findings[0].message,
        format!(
            "graph has more than {VALIDATE_RELATIONSHIP_LIMIT} relationships; validation truncated"
        )
    );
    assert_eq!(findings[1].id, "finding-ontology-z");
    assert_eq!(
        findings[1].message,
        "relationship predicate `missing` is not declared by ontology `ontology`"
    );
    assert_eq!(findings[1].severity, OntologyValidationSeverity::Warning);
    assert_eq!(findings[1].provenance.source, "ontology:ontology");
    assert_eq!(
        findings[1].provenance.actor.id.to_string(),
        "engram-ontology-validator"
    );
    assert_eq!(
        findings[1].provenance.method.as_deref(),
        Some("ontology_validation")
    );
    assert_eq!(findings[1].detected_at, now);
    assert_eq!(findings[1].provenance.observed_at, now);
    assert_eq!(findings[1].provenance.confidence, Some(1.0));
    assert!(validate_visible_graph(&ontology, &[], &[], false, now).is_empty());
}

#[test]
fn canonical_validation_observes_exact_scan_limit() {
    let rows=(0..=VALIDATE_RELATIONSHIP_LIMIT).map(|i|record::<KnowledgeRelationship>(json!({"id":format!("{i:05}"),"predicate":"missing","subject":{"id":"left"},"object":{"id":"right"},"scope":{"tenant":"tenant"}}))).collect::<Vec<_>>();
    let now: Timestamp = "2026-11-17T00:00:00Z".parse().unwrap();
    let findings = validate_visible_graph(&"ontology".into(), &[], &rows, true, now);
    assert_eq!(findings.len(), VALIDATE_RELATIONSHIP_LIMIT + 1);
    assert!(
        findings
            .iter()
            .any(|f| f.id == "finding-ontology-truncated")
    );
    assert!(
        !findings
            .iter()
            .any(|f| f.id == format!("finding-ontology-{:05}", VALIDATE_RELATIONSHIP_LIMIT))
    );
}
