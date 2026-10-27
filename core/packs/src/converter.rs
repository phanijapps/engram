//! Offline pack converter (plan S5, DEC-P02): parse committed Turtle or
//! RDF/XML pack bytes into structural domain records — one `Ontology`, its
//! `OntologyClass` set with `rdfs:subClassOf` hierarchy, and its
//! `OntologyProperty` set with domain/range wiring.
//!
//! Pure by construction: bytes in, records out. No network, no filesystem,
//! no clock (the caller supplies `created_at`). Record ids derive
//! deterministically from the pack id and term URI, so re-running an install
//! upserts the same rows (idempotent convergence, RISK-003).

use serde::Serialize;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, Id, Metadata, Ontology, OntologyClass,
    OntologyClassId, OntologyLanguage, OntologyProperty, OntologyPropertyId, OntologyStatus,
    OntologyTermStatus, Policy, Provenance, Retention, Scope, Sensitivity, Timestamp, Visibility,
};
use oxrdf::{NamedOrBlankNodeRef, Quad, TermRef};
use oxrdfio::{RdfFormat, RdfParser};
use sha2::{Digest, Sha256};

use crate::catalogue::PackFormat;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";

/// The converted pack: the structural records a pack install writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConvertedPack {
    pub ontology: Ontology,
    pub classes: Vec<OntologyClass>,
    pub properties: Vec<OntologyProperty>,
}

/// Convert pinned pack bytes into structural records.
///
/// `pack_sha256` is verified by the caller (the fetch layer owns the pin
/// check); it is recorded into the ontology metadata so facade reads can
/// report pack provenance without re-hashing.
pub fn convert_pack(
    pack_id: &str,
    pack_url: &str,
    format: PackFormat,
    bytes: &[u8],
    pack_sha256: &str,
    scope: &Scope,
    now: DateTime<Utc>,
) -> Result<ConvertedPack, String> {
    let rdf_format = match format {
        PackFormat::Turtle => RdfFormat::Turtle,
        PackFormat::RdfXml => RdfFormat::RdfXml,
    };
    let mut quads: Vec<Quad> = Vec::new();
    for quad in RdfParser::from_format(rdf_format).for_slice(bytes) {
        let quad = quad.map_err(|e| format!("parse pack {pack_id}: {e}"))?;
        quads.push(quad);
    }

    // Pass 1: collect class URIs, property URIs, labels, comments, wiring.
    let mut class_uris: Vec<String> = Vec::new();
    let mut property_kinds: BTreeMap<String, PropertyKind> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut comments: BTreeMap<String, String> = BTreeMap::new();
    let mut subclasses: Vec<(String, String)> = Vec::new();
    let mut domains: BTreeMap<String, String> = BTreeMap::new();
    let mut ranges: BTreeMap<String, String> = BTreeMap::new();
    let mut saw_owl_construct = false;

    for quad in &quads {
        let (s, p, o) = (
            quad.subject.as_ref(),
            quad.predicate.as_ref(),
            quad.object.as_ref(),
        );
        let subject_iri = match s {
            NamedOrBlankNodeRef::NamedNode(n) => n.as_str(),
            _ => continue,
        };
        let predicate = p.as_str();
        let object_iri = match o {
            TermRef::NamedNode(n) => Some(n.as_str()),
            TermRef::Literal(l) => Some(l.value()),
            _ => None,
        };
        match predicate {
            RDF_TYPE => {
                let object = object_iri.unwrap_or_default();
                match object {
                    OWL_CLASS | RDFS_CLASS => {
                        saw_owl_construct |= object == OWL_CLASS;
                        if !class_uris.iter().any(|u| u == subject_iri) {
                            class_uris.push(subject_iri.to_owned());
                        }
                    }
                    OWL_OBJECT_PROPERTY => {
                        saw_owl_construct = true;
                        property_kinds.insert(subject_iri.to_owned(), PropertyKind::Object);
                    }
                    OWL_DATATYPE_PROPERTY => {
                        saw_owl_construct = true;
                        property_kinds.insert(subject_iri.to_owned(), PropertyKind::Data);
                    }
                    OWL_ANNOTATION_PROPERTY => {
                        saw_owl_construct = true;
                        property_kinds
                            .entry(subject_iri.to_owned())
                            .or_insert(PropertyKind::Annotation);
                    }
                    RDF_PROPERTY => {
                        property_kinds
                            .entry(subject_iri.to_owned())
                            .or_insert(PropertyKind::Object);
                    }
                    "http://www.w3.org/2002/07/owl#Ontology"
                    | "http://www.w3.org/2002/07/owl#Restriction" => saw_owl_construct = true,
                    _ => {}
                }
            }
            RDFS_SUBCLASS_OF => {
                if let Some(object) = object_iri {
                    subclasses.push((subject_iri.to_owned(), object.to_owned()));
                }
            }
            RDFS_SUBPROPERTY_OF | RDFS_DOMAIN => {
                if predicate == RDFS_DOMAIN
                    && let Some(object) = object_iri
                {
                    domains.insert(subject_iri.to_owned(), object.to_owned());
                }
            }
            RDFS_RANGE => {
                if let Some(object) = object_iri {
                    ranges.insert(subject_iri.to_owned(), object.to_owned());
                }
            }
            RDFS_LABEL => {
                if let Some(object) = object_iri
                    && !labels.contains_key(subject_iri)
                {
                    labels.insert(subject_iri.to_owned(), object.to_owned());
                }
            }
            RDFS_COMMENT => {
                if let Some(object) = object_iri
                    && !comments.contains_key(subject_iri)
                {
                    comments.insert(subject_iri.to_owned(), object.to_owned());
                }
            }
            _ => {}
        }
        if predicate.starts_with("http://www.w3.org/2002/07/owl#") {
            saw_owl_construct = true;
        }
    }

    // "It parsed" is not "it installed": a pack that declares neither a
    // class nor a property is empty, whatever its triple count says.
    if class_uris.is_empty() && property_kinds.is_empty() {
        return Err(format!(
            "pack {pack_id} declares no class and no property — refusing an empty install"
        ));
    }

    let mut class_uris = class_uris;
    class_uris.sort();
    let mut property_uris: Vec<String> = property_kinds.keys().cloned().collect();
    property_uris.sort();

    let hierarchy_edges: Vec<_> = subclasses
        .iter()
        .filter(|(child, parent)| {
            class_uris.iter().any(|u| u == child) && class_uris.iter().any(|u| u == parent)
        })
        .cloned()
        .collect();

    let prov = pack_provenance(pack_id, now);
    let pol = pack_policy();
    let class_id = |uri: &str| OntologyClassId::from(term_id(pack_id, "class", uri));
    let ontology_id = engram_domain::OntologyId::from(format!("pack-{pack_id}"));

    let classes: Vec<OntologyClass> = class_uris
        .iter()
        .map(|uri| OntologyClass {
            id: class_id(uri),
            ontology_id: ontology_id.clone(),
            uri: uri.clone(),
            label: labels.get(uri).cloned().unwrap_or_else(|| local_name(uri)),
            description: comments.get(uri).cloned(),
            parent_class_ids: hierarchy_edges
                .iter()
                .filter(|(child, _)| child == uri)
                .map(|(_, parent)| class_id(parent))
                .collect(),
            concept_refs: Vec::new(),
            status: OntologyTermStatus::Active,
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        })
        .collect();

    let properties: Vec<OntologyProperty> = property_uris
        .iter()
        .map(|uri| OntologyProperty {
            id: OntologyPropertyId::from(term_id(pack_id, "property", uri)),
            ontology_id: ontology_id.clone(),
            uri: uri.clone(),
            label: labels.get(uri).cloned().unwrap_or_else(|| local_name(uri)),
            kind: property_kinds[uri].domain_kind(),
            domain_class_id: domains
                .get(uri)
                .filter(|d| class_uris.iter().any(|u| u == *d))
                .map(|d| class_id(d)),
            range_class_id: ranges
                .get(uri)
                .filter(|r| class_uris.iter().any(|u| u == *r))
                .map(|r| class_id(r)),
            datatype: property_kinds[uri]
                .is_data()
                .then(|| ranges.get(uri).cloned())
                .flatten(),
            inverse_property_id: None,
            status: OntologyTermStatus::Active,
            provenance: prov.clone(),
            created_at: now,
            updated_at: None,
            metadata: None,
        })
        .collect();

    let mut metadata = Metadata::new();
    metadata.insert("packId".to_owned(), serde_json::json!(pack_id));
    metadata.insert("packSha256".to_owned(), serde_json::json!(pack_sha256));
    metadata.insert(
        "packClasses".to_owned(),
        serde_json::json!(classes.len() as u64),
    );
    metadata.insert(
        "packProperties".to_owned(),
        serde_json::json!(properties.len() as u64),
    );
    metadata.insert(
        "packHierarchyEdges".to_owned(),
        serde_json::json!(hierarchy_edges.len() as u64),
    );
    let computed_sha = hex(&Sha256::digest(bytes));
    metadata.insert(
        "packBytesSha256".to_owned(),
        serde_json::json!(computed_sha),
    );

    let ontology = Ontology {
        id: ontology_id,
        uri: pack_url.to_owned(),
        name: format!("pack:{pack_id}"),
        scope: scope.clone(),
        language: if saw_owl_construct {
            OntologyLanguage::Owl
        } else {
            OntologyLanguage::Rdfs
        },
        version: format!("pinned:{}", &pack_sha256[..12.min(pack_sha256.len())]),
        status: OntologyStatus::Active,
        imports: Vec::new(),
        policy: pol,
        provenance: prov,
        created_at: now,
        updated_at: None,
        metadata: Some(metadata),
    };

    Ok(ConvertedPack {
        ontology,
        classes,
        properties,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyKind {
    Object,
    Data,
    Annotation,
}

impl PropertyKind {
    fn domain_kind(self) -> engram_domain::OntologyPropertyKind {
        match self {
            Self::Object => engram_domain::OntologyPropertyKind::Object,
            Self::Data => engram_domain::OntologyPropertyKind::Data,
            Self::Annotation => engram_domain::OntologyPropertyKind::Annotation,
        }
    }
    fn is_data(self) -> bool {
        matches!(self, Self::Data)
    }
}

/// Stable record id for one pack term: `{pack}:class:{uri}` — deterministic,
/// so re-installs upsert the same row instead of duplicating it.
fn term_id(pack_id: &str, kind: &str, uri: &str) -> String {
    format!("{pack_id}:{kind}:{uri}")
}

fn local_name(uri: &str) -> String {
    let no_fragment = uri.rsplit('#').next().unwrap_or(uri);
    let last = no_fragment.rsplit('/').next().unwrap_or(no_fragment);
    if last.is_empty() {
        uri.to_owned()
    } else {
        last.to_owned()
    }
}

fn pack_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn pack_provenance(pack_id: &str, now: DateTime<Utc>) -> Provenance {
    Provenance {
        source: format!("engram-pack:{pack_id}"),
        actor: Actor {
            id: engram_domain::ActorId::from("engram-packs"),
            kind: ActorKind::Tool,
            display_name: Some("engram pack engine".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: None,
        method: Some("pinned-pack-conversion".to_owned()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
