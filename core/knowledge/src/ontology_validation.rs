//! Canonical advisory ontology findings; adapters own scoped, bounded reads.
use engram_domain::*;
use std::collections::HashSet;
pub const VALIDATE_RELATIONSHIP_LIMIT: usize = 5_000;
/// Inputs are already visible to the caller; truncation describes the bounded physical scan.
pub fn validate_visible_graph(
    ontology_id: &OntologyId,
    properties: &[OntologyProperty],
    relationships: &[KnowledgeRelationship],
    truncated: bool,
    now: Timestamp,
) -> Vec<OntologyValidationFinding> {
    let declared: HashSet<String> = properties
        .iter()
        .flat_map(|p| [p.label.to_lowercase(), p.uri.to_lowercase()])
        .collect();
    let mut findings = Vec::new();
    for relationship in relationships.iter().take(VALIDATE_RELATIONSHIP_LIMIT) {
        if declared.contains(&relationship.predicate.to_lowercase()) {
            continue;
        }
        findings.push(OntologyValidationFinding {
            id: format!("finding-{ontology_id}-{}", relationship.id),
            ontology_id: ontology_id.clone(),
            severity: OntologyValidationSeverity::Warning,
            code: "undeclared_predicate".to_owned(),
            message: format!(
                "relationship predicate `{}` is not declared by ontology `{ontology_id}`",
                relationship.predicate
            ),
            target: Some(relationship.subject.clone()),
            axiom_id: None,
            provenance: validation_provenance(ontology_id, now),
            detected_at: now,
        });
    }
    if truncated {
        findings.push(OntologyValidationFinding {
            id: format!("finding-{ontology_id}-truncated"),
            ontology_id: ontology_id.clone(),
            severity: OntologyValidationSeverity::Info,
            code: "validation_truncated".to_owned(),
            message: format!(
                "graph has more than {limit} relationships; validation truncated",
                limit = VALIDATE_RELATIONSHIP_LIMIT
            ),
            target: None,
            axiom_id: None,
            provenance: validation_provenance(ontology_id, now),
            detected_at: now,
        });
    }
    findings.sort_by(|left, right| left.id.cmp(&right.id));
    findings
}
/// Builds the advisory provenance stamped on every validation finding. Validation
/// is deterministic and carries no input evidence, so a fixed system actor is used.
fn validation_provenance(ontology_id: &OntologyId, now: Timestamp) -> Provenance {
    Provenance {
        source: format!("ontology:{ontology_id}"),
        actor: Actor {
            id: Id::from("engram-ontology-validator"),
            kind: ActorKind::System,
            display_name: Some("Ontology validator".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("ontology_validation".to_owned()),
    }
}
