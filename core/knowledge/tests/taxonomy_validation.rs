use engram_domain::*;
use engram_knowledge::validate_taxonomy_proposal;

fn ts() -> Timestamp {
    "2026-07-02T12:00:00Z".parse().expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-taxonomy"),
        kind: ActorKind::System,
        display_name: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "taxonomy-validation-test".to_owned(),
        actor: actor(),
        observed_at: ts(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("proposal_validation".to_owned()),
    }
}

fn proposal(status: TaxonomyProposalStatus, reviewer: Option<Actor>) -> TaxonomyProposal {
    TaxonomyProposal {
        id: "proposal-1".to_owned(),
        scheme_id: Id::from("scheme-1"),
        status,
        changes: Vec::new(),
        validation: None,
        semantic_drift: Vec::new(),
        proposer: actor(),
        reviewer,
        provenance: provenance(),
        created_at: ts(),
        reviewed_at: None,
        metadata: None,
    }
}

fn concept(id: &str, label: &str) -> Concept {
    Concept {
        id: Id::from(id),
        uri: format!("urn:concept:{id}"),
        scheme_id: Id::from("scheme-1"),
        pref_label: ConceptLabel {
            value: label.to_owned(),
            language: Some("en".to_owned()),
        },
        alt_labels: Vec::new(),
        definition: None,
        notation: None,
        status: ConceptStatus::Active,
        provenance: provenance(),
        created_at: ts(),
        updated_at: None,
    }
}

fn relation(
    id: &str,
    subject: &str,
    predicate: ConceptRelationKind,
    object: &str,
) -> ConceptRelation {
    ConceptRelation {
        id: id.to_owned(),
        scheme_id: Id::from("scheme-1"),
        subject_id: Id::from(subject),
        predicate,
        object_id: Id::from(object),
        provenance: provenance(),
        created_at: ts(),
    }
}

fn finding_codes(report: &TaxonomyValidationReport) -> Vec<String> {
    report
        .findings
        .iter()
        .map(|finding| finding.code.clone())
        .collect()
}

#[test]
fn clean_taxonomy_proposal_passes() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Proposed, None),
        &[concept("runtime", "Runtime"), concept("adapter", "Adapter")],
        &[relation(
            "runtime-broader-adapter",
            "runtime",
            ConceptRelationKind::Related,
            "adapter",
        )],
    );

    assert_eq!(report.status, TaxonomyValidationStatus::Passed);
    assert!(report.findings.is_empty());
}

#[test]
fn duplicate_preferred_labels_fail() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Proposed, None),
        &[concept("rust", "Rust"), concept("rust-duplicate", "rust")],
        &[],
    );

    assert_eq!(report.status, TaxonomyValidationStatus::Failed);
    assert!(finding_codes(&report).contains(&"duplicate_pref_label".to_owned()));
}

#[test]
fn relation_to_missing_concept_fails() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Proposed, None),
        &[concept("runtime", "Runtime")],
        &[relation(
            "runtime-broader-missing",
            "runtime",
            ConceptRelationKind::Broader,
            "missing",
        )],
    );

    assert!(finding_codes(&report).contains(&"relation_missing_object".to_owned()));
}

#[test]
fn broader_cycles_fail() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Proposed, None),
        &[concept("a", "A"), concept("b", "B")],
        &[
            relation("a-broader-b", "a", ConceptRelationKind::Broader, "b"),
            relation("b-broader-a", "b", ConceptRelationKind::Broader, "a"),
        ],
    );

    assert!(finding_codes(&report).contains(&"broader_cycle".to_owned()));
}

#[test]
fn merged_proposal_without_reviewer_fails() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Merged, None),
        &[concept("runtime", "Runtime")],
        &[],
    );

    assert!(finding_codes(&report).contains(&"merge_requires_reviewer".to_owned()));
}

#[test]
fn merged_proposal_with_reviewer_passes() {
    let report = validate_taxonomy_proposal(
        &proposal(TaxonomyProposalStatus::Merged, Some(actor())),
        &[concept("runtime", "Runtime")],
        &[],
    );

    assert_eq!(report.status, TaxonomyValidationStatus::Passed);
}
