//! Pure validation for governed taxonomy proposals.
//!
//! This module validates proposed concept and relation state without reading or
//! writing a repository. Storage adapters persist accepted schemes, concepts,
//! and relations; proposal governance remains a typed behavior step above them.

use std::collections::{BTreeMap, BTreeSet};

use engram_domain::*;

/// Validates proposed taxonomy state and returns a deterministic report.
///
/// The validator fails proposals that would introduce duplicate preferred
/// labels, relations to missing concepts, broader/narrower cycles, or merged
/// proposals without an explicit reviewer. It does not mutate taxonomy records.
pub fn validate_taxonomy_proposal(
    proposal: &TaxonomyProposal,
    concepts: &[Concept],
    relations: &[ConceptRelation],
) -> TaxonomyValidationReport {
    let mut findings = Vec::new();
    if proposal.status == TaxonomyProposalStatus::Merged && proposal.reviewer.is_none() {
        findings.push(finding(
            proposal,
            "merge_requires_reviewer",
            "merged taxonomy proposals require an explicit reviewer",
            Some("proposal"),
            Some(&proposal.id),
        ));
    }

    duplicate_label_findings(proposal, concepts, &mut findings);
    relation_findings(proposal, concepts, relations, &mut findings);

    TaxonomyValidationReport {
        id: format!("validation-{}", proposal.id),
        proposal_id: proposal.id.clone(),
        status: if findings.is_empty() {
            TaxonomyValidationStatus::Passed
        } else {
            TaxonomyValidationStatus::Failed
        },
        findings,
        checked_at: proposal.created_at,
        provenance: proposal.provenance.clone(),
    }
}

fn duplicate_label_findings(
    proposal: &TaxonomyProposal,
    concepts: &[Concept],
    findings: &mut Vec<TaxonomyValidationFinding>,
) {
    let mut labels: BTreeMap<(String, Option<String>), &Concept> = BTreeMap::new();
    for concept in concepts
        .iter()
        .filter(|concept| concept.scheme_id == proposal.scheme_id)
        .filter(|concept| concept.status != ConceptStatus::Rejected)
    {
        let key = (
            concept.pref_label.value.trim().to_lowercase(),
            concept.pref_label.language.clone(),
        );
        if let Some(existing) = labels.insert(key, concept) {
            findings.push(finding(
                proposal,
                "duplicate_pref_label",
                "preferred labels must be unique within a concept scheme and language",
                Some("concept"),
                Some(&format!("{},{}", existing.id, concept.id)),
            ));
        }
    }
}

fn relation_findings(
    proposal: &TaxonomyProposal,
    concepts: &[Concept],
    relations: &[ConceptRelation],
    findings: &mut Vec<TaxonomyValidationFinding>,
) {
    let concept_ids = concepts
        .iter()
        .filter(|concept| concept.scheme_id == proposal.scheme_id)
        .map(|concept| concept.id.to_string())
        .collect::<BTreeSet<_>>();

    for relation in relations
        .iter()
        .filter(|relation| relation.scheme_id == proposal.scheme_id)
    {
        if !concept_ids.contains(relation.subject_id.as_str()) {
            findings.push(finding(
                proposal,
                "relation_missing_subject",
                "concept relation subject must exist in the proposal scheme",
                Some("concept_relation"),
                Some(&relation.id),
            ));
        }
        if !concept_ids.contains(relation.object_id.as_str()) {
            findings.push(finding(
                proposal,
                "relation_missing_object",
                "concept relation object must exist in the proposal scheme",
                Some("concept_relation"),
                Some(&relation.id),
            ));
        }
    }

    if has_broader_cycle(relations, &proposal.scheme_id) {
        findings.push(finding(
            proposal,
            "broader_cycle",
            "broader/narrower taxonomy relations must not form a cycle",
            Some("concept_scheme"),
            Some(proposal.scheme_id.as_str()),
        ));
    }
}

fn has_broader_cycle(relations: &[ConceptRelation], scheme_id: &ConceptSchemeId) -> bool {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for relation in relations
        .iter()
        .filter(|relation| relation.scheme_id == *scheme_id)
    {
        match relation.predicate {
            ConceptRelationKind::Broader => graph
                .entry(relation.subject_id.to_string())
                .or_default()
                .push(relation.object_id.to_string()),
            ConceptRelationKind::Narrower => graph
                .entry(relation.object_id.to_string())
                .or_default()
                .push(relation.subject_id.to_string()),
            ConceptRelationKind::Related => {}
        }
    }

    graph
        .keys()
        .any(|node| visits_cycle(node, &graph, &mut BTreeSet::new(), &mut BTreeSet::new()))
}

fn visits_cycle(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_owned()) {
        return true;
    }
    for next in graph.get(node).into_iter().flatten() {
        if visits_cycle(next, graph, visiting, visited) {
            return true;
        }
    }
    visiting.remove(node);
    visited.insert(node.to_owned());
    false
}

fn finding(
    proposal: &TaxonomyProposal,
    code: &str,
    message: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) -> TaxonomyValidationFinding {
    TaxonomyValidationFinding {
        id: format!("{}-{code}", proposal.id),
        severity: TaxonomyFindingSeverity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
        target_type: target_type.map(str::to_owned),
        target_id: target_id.map(str::to_owned),
        provenance: proposal.provenance.clone(),
        detected_at: proposal.created_at,
    }
}
