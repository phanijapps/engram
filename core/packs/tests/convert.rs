//! Converter integration tests (TASK-006): committed fixture bytes in both
//! pinned formats parse into structural records with hierarchy — no network.

use chrono::{TimeZone, Utc};
use engram_domain::Scope;
use engram_packs::{PackFormat, convert_pack};

fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    Utc.timestamp_opt(0, 0).unwrap()
}

fn scope() -> Scope {
    Scope {
        tenant: "default".to_owned(),
        subject: None,
        workspace: Some("convert-tests".to_owned()),
        session: None,
        environment: None,
    }
}

fn bank_fixture() -> Vec<u8> {
    std::fs::read("tests/fixtures/bank-fixture.ttl").expect("committed turtle fixture")
}

fn travel_fixture() -> Vec<u8> {
    std::fs::read("tests/fixtures/travel-fixture.rdf").expect("committed rdf/xml fixture")
}

#[test]
fn convert_turtle_fixture_parses_classes_properties_hierarchy() {
    let pack = convert_pack(
        "bank-fixture",
        "fixture://bank.ttl",
        PackFormat::Turtle,
        &bank_fixture(),
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .expect("turtle fixture converts");
    let loan = pack
        .classes
        .iter()
        .find(|c| c.label == "Loan")
        .expect("Loan class parsed");
    assert_eq!(
        loan.parent_class_ids.len(),
        1,
        "Loan has its subClassOf parent"
    );
    let debt = pack
        .classes
        .iter()
        .find(|c| c.label == "Debt Instrument")
        .expect("Debt Instrument parsed");
    assert_eq!(
        loan.parent_class_ids[0], debt.id,
        "hierarchy edge Loan→Debt"
    );
    assert!(pack.classes.len() >= 6, "all fixture classes parsed");
    assert!(pack.properties.len() >= 4, "all fixture properties parsed");
    let principal = pack
        .properties
        .iter()
        .find(|p| p.label == "principal amount")
        .expect("datatype property parsed");
    assert!(
        principal.datatype.is_some(),
        "xsd range captured as datatype"
    );
    let has_borrower = pack
        .properties
        .iter()
        .find(|p| p.label == "has borrower")
        .expect("object property parsed");
    assert!(has_borrower.domain_class_id.is_some(), "domain wired");
    assert!(has_borrower.range_class_id.is_some(), "range wired");
}

#[test]
fn convert_rdf_xml_fixture_parses_same_shape() {
    let pack = convert_pack(
        "travel-fixture",
        "fixture://travel.rdf",
        PackFormat::RdfXml,
        &travel_fixture(),
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .expect("rdf/xml fixture converts");
    let hotel = pack
        .classes
        .iter()
        .find(|c| c.label == "Hotel")
        .expect("Hotel parsed from RDF/XML");
    assert_eq!(
        hotel.parent_class_ids.len(),
        1,
        "Hotel→LodgingBusiness edge"
    );
    let resort = pack
        .classes
        .iter()
        .find(|c| c.label == "Resort")
        .expect("Resort parsed");
    assert_eq!(resort.parent_class_ids.len(), 1, "Resort→Hotel edge");
    assert!(pack.properties.len() >= 3, "RDF/XML properties parsed");
}

#[test]
fn convert_output_is_deterministic_for_identical_input() {
    let a = convert_pack(
        "bank-fixture",
        "fixture://bank.ttl",
        PackFormat::Turtle,
        &bank_fixture(),
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .unwrap();
    let b = convert_pack(
        "bank-fixture",
        "fixture://bank.ttl",
        PackFormat::Turtle,
        &bank_fixture(),
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .unwrap();
    let sa = serde_json::to_string(&a).unwrap();
    let sb = serde_json::to_string(&b).unwrap();
    assert_eq!(sa, sb, "two conversions of identical bytes are identical");
    // Record ids are stable, so re-installs upsert the same rows.
    assert_eq!(a.classes[0].id, b.classes[0].id);
}

#[test]
fn convert_rejects_pack_that_declares_no_terms() {
    let empty = b"@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n<urn:x> rdfs:label \"only a label\" .\n";
    let err = convert_pack(
        "empty-pack",
        "fixture://empty.ttl",
        PackFormat::Turtle,
        empty,
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .expect_err("empty packs must be refused");
    assert!(err.contains("declares no class"), "got: {err}");
}

#[test]
fn convert_labels_fall_back_to_uri_local_name() {
    let turtle = b"@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<https://example.org/v/UnlabeledThing> a owl:Class .\n";
    let pack = convert_pack(
        "unlabeled",
        "fixture://unlabeled.ttl",
        PackFormat::Turtle,
        turtle,
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .unwrap();
    assert_eq!(pack.classes[0].label, "UnlabeledThing");
}

#[test]
fn convert_ontology_metadata_carries_counts() {
    let pack = convert_pack(
        "bank-fixture",
        "fixture://bank.ttl",
        PackFormat::Turtle,
        &bank_fixture(),
        "0".repeat(64).as_str(),
        &scope(),
        fixed_now(),
    )
    .unwrap();
    let metadata = pack.ontology.metadata.as_ref().expect("counts recorded");
    assert_eq!(
        metadata.get("packClasses").and_then(|v| v.as_u64()),
        Some(pack.classes.len() as u64)
    );
    assert_eq!(
        metadata.get("packProperties").and_then(|v| v.as_u64()),
        Some(pack.properties.len() as u64)
    );
    assert!(
        metadata.get("packHierarchyEdges").and_then(|v| v.as_u64()) == Some(4),
        "hierarchy rows present: {:?}",
        metadata
    );
}
