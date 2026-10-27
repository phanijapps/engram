//! Deterministic structural-record builders for slice/converter tests.
//!
//! Never used by production code paths — tests only (cfg(test) gated module
//! plus this shared helper file compiled only under dev).

#[allow(dead_code)]
use chrono::{TimeZone, Utc};
use engram_domain::{
    Actor, ActorKind, OntologyClass, OntologyClassId, OntologyProperty, OntologyPropertyId,
    Provenance, Scope,
};

#[allow(dead_code)]
pub fn fixed_now() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(0, 0).unwrap()
}

#[allow(dead_code)]
pub fn test_scope() -> Scope {
    Scope {
        tenant: "default".to_owned(),
        subject: None,
        workspace: Some("pack-tests".to_owned()),
        session: None,
        environment: None,
    }
}

fn prov() -> Provenance {
    Provenance {
        source: "test".to_owned(),
        actor: Actor {
            id: engram_domain::ActorId::from("test"),
            kind: ActorKind::Tool,
            display_name: None,
            metadata: None,
        },
        observed_at: fixed_now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: None,
        method: None,
    }
}

/// `n` classes labeled `class-0..class-n-1` where class-i is a subclass of
/// class-(i+1) — a chain, so class-(n-1) has n-1 descendants and ranking is
/// fully predictable: the highest index first.
pub fn synthetic_classes(n: usize) -> Vec<OntologyClass> {
    (0..n)
        .map(|i| OntologyClass {
            id: OntologyClassId::from(format!("synthetic:class:{i}")),
            ontology_id: engram_domain::OntologyId::from("pack-synthetic"),
            uri: format!("https://synthetic.test/v/class-{i}"),
            label: format!("class-{i}"),
            description: None,
            parent_class_ids: if i + 1 < n {
                vec![OntologyClassId::from(format!("synthetic:class:{}", i + 1))]
            } else {
                Vec::new()
            },
            concept_refs: Vec::new(),
            status: engram_domain::OntologyTermStatus::Active,
            provenance: prov(),
            created_at: fixed_now(),
            updated_at: None,
            metadata: None,
        })
        .collect()
}

/// `n` object properties labeled `prop-0..prop-n-1` with ascending URIs.
pub fn synthetic_properties(n: usize) -> Vec<OntologyProperty> {
    (0..n)
        .map(|i| OntologyProperty {
            id: OntologyPropertyId::from(format!("synthetic:property:{i}")),
            ontology_id: engram_domain::OntologyId::from("pack-synthetic"),
            uri: format!("https://synthetic.test/v/prop-{i:04}"),
            label: format!("prop-{i}"),
            kind: engram_domain::OntologyPropertyKind::Object,
            domain_class_id: None,
            range_class_id: None,
            datatype: None,
            inverse_property_id: None,
            status: engram_domain::OntologyTermStatus::Active,
            provenance: prov(),
            created_at: fixed_now(),
            updated_at: None,
            metadata: None,
        })
        .collect()
}
