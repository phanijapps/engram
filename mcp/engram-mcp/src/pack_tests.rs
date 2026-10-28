//! Pack subcommand tests (TASK-008 / plan S7): offline second install,
//! first-fetch mismatch hard error, facade row counts, sliced-vs-available
//! counts, advisory non-blocking findings, two-project isolation, and
//! offline widening regeneration.

use std::path::{Path, PathBuf};

use engram_domain::{ConceptSchemeId, OntologyId, Scope};
use engram_packs::{Catalogue, SliceSize};
use futures::executor::block_on;

use crate::config::McpConfig;
use crate::pack;
use crate::scope;

/// Committed fixture bytes (shared with engram-packs converter tests).
fn bank_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/packs/tests/fixtures/bank-fixture.ttl")
}

fn travel_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/packs/tests/fixtures/travel-fixture.rdf")
}

fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A dev catalogue override pointing file:// at the committed fixtures.
fn fixture_catalogue(dir: &Path) -> PathBuf {
    let bank = bank_fixture();
    let travel = travel_fixture();
    let catalogue = serde_json::json!({
        "_header": {
            "description": "fixture catalogue for tests",
            "excluded_language_bases": { "reason": "test exclusion record" },
            "pin_run": {}, "license_policy": ""
        },
        "entries": [
            {
                "id": "bank-fixture",
                "name": "Bank Fixture",
                "description": "fixture banking vocabulary",
                "domain": "finance",
                "url": format!("file://{}", bank.display()),
                "format": "turtle",
                "license": "CC-BY-4.0",
                "license_source": "pack-bytes",
                "sha256": sha256_of(&bank)
            },
            {
                "id": "travel-fixture",
                "name": "Travel Fixture",
                "description": "fixture travel vocabulary",
                "domain": "general",
                "url": format!("file://{}", travel.display()),
                "format": "rdf-xml",
                "license": "CC-BY-4.0",
                "license_source": "pack-bytes",
                "sha256": sha256_of(&travel)
            }
        ]
    });
    let path = dir.join("catalogue.json");
    std::fs::write(&path, catalogue.to_string()).unwrap();
    path
}

fn load_catalogue(dir: &Path) -> engram_packs::Catalogue {
    let text = std::fs::read_to_string(fixture_catalogue(dir)).unwrap();
    engram_packs::Catalogue::from_json(&text).unwrap()
}

fn mcp_config(storage: &Path, project: &str) -> McpConfig {
    McpConfig::from_args(&[
        "--storage".to_owned(),
        storage.display().to_string(),
        "--project".to_owned(),
        project.to_owned(),
        "--no-vector".to_owned(),
    ])
    .unwrap()
}

fn scope_for(project: &str) -> Scope {
    scope::resolve_scope(None, None, None, project)
}

fn launch_ontology_json(storage: &Path, project: &str) -> String {
    std::fs::read_to_string(pack::launch_dir(storage, &scope_for(project)).join("ontology.json"))
        .unwrap()
}

#[test]
fn pack_install_fetches_once_then_offline_second_install_is_identical() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-a");

    let first = pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .expect("first install (fetch once)");
    assert!(
        first.contains("source: fetch (sha256 verified"),
        "first install reports the verified fetch: {first}"
    );

    let before = std::fs::read(pack::cache_dir(&storage, "bank-fixture").join("pack.ttl")).unwrap();
    let second = pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .expect("second install from cache");
    assert!(
        second.contains("source: cache (zero fetches)"),
        "second install hits the cache only: {second}"
    );
    let after = std::fs::read(pack::cache_dir(&storage, "bank-fixture").join("pack.ttl")).unwrap();
    assert_eq!(before, after, "cached bytes are immutable");

    // Launch configs regenerate byte-identically (AC-005).
    let config_text = launch_ontology_json(&storage, "proj-a");
    let _ = pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    );
    assert_eq!(config_text, launch_ontology_json(&storage, "proj-a"));
}

#[test]
fn pack_install_rejects_mismatched_first_fetch_and_installs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let bank = bank_fixture();
    let tampered = serde_json::json!({
        "_header": {
            "description": "tampered pin", "excluded_language_bases": { "reason": "r" },
            "pin_run": {}, "license_policy": ""
        },
        "entries": [{
            "id": "bank-fixture", "name": "Bank", "description": "", "domain": "finance",
            "url": format!("file://{}", bank.display()),
            "format": "turtle", "license": "CC-BY-4.0", "license_source": "pack-bytes",
            "sha256": format!("0{}", "0".repeat(63))
        }]
    });
    let catalogue_path = dir.path().join("tampered.json");
    std::fs::write(&catalogue_path, tampered.to_string()).unwrap();
    let entry =
        engram_packs::Catalogue::from_json(&std::fs::read_to_string(&catalogue_path).unwrap())
            .unwrap();
    let config = mcp_config(&storage, "proj-a");
    let err = pack::install_pack(&config, &entry, "bank-fixture", SliceSize::Default, None)
        .expect_err("mismatched first fetch must hard-error");
    assert!(
        err.contains("sha256 mismatch at first fetch"),
        "error names the failure: {err}"
    );
    assert!(err.contains("bank-fixture"), "error names the entry: {err}");
    assert!(
        err.contains("RISK-001") || err.contains("re-pin"),
        "error names the re-pin procedure: {err}"
    );
    assert!(
        !pack::cache_dir(&storage, "bank-fixture").exists(),
        "nothing divergent is installed or cached"
    );
    assert!(
        !pack::launch_dir(&storage, &scope_for("proj-a"))
            .join("ontology.json")
            .exists(),
        "no launch config is emitted for a refused install"
    );
}

#[test]
fn pack_install_writes_full_structural_rows_with_counts() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-counts");

    pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .unwrap();

    let provider = crate::bootstrap::open_provider(&config).unwrap();
    let project_scope = scope_for("proj-counts");
    let ontology = block_on(
        provider
            .require_ontology()
            .unwrap()
            .get_ontology(&OntologyId::from("pack-bank-fixture"), &project_scope),
    )
    .unwrap()
    .expect("ontology record readable via facade");
    let metadata = ontology.metadata.as_ref().unwrap();
    assert_eq!(
        metadata.get("packClasses").unwrap().as_u64().unwrap(),
        6,
        "full pack class count in the ontology record"
    );
    assert_eq!(metadata.get("packProperties").unwrap().as_u64().unwrap(), 4);
    assert_eq!(
        metadata
            .get("packHierarchyEdges")
            .unwrap()
            .as_u64()
            .unwrap(),
        4
    );

    // Taxonomy mirror read-back through the existing port, with counts.
    let concepts = block_on(
        provider
            .require_taxonomy()
            .unwrap()
            .list_concepts(&ConceptSchemeId::from("pack-bank-fixture"), &project_scope),
    )
    .unwrap();
    assert_eq!(concepts.len(), 6, "one concept per class, readable");
    assert!(
        concepts.iter().any(|c| c.pref_label.value == "Loan"),
        "class labels survive into the taxonomy mirror"
    );
}

#[test]
fn pack_install_reports_sliced_vs_available_counts() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-slice");
    let report = pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .unwrap();
    assert!(
        report.contains("classes 6/6 available, predicates 4/4 available"),
        "sliced-vs-available counts are reported, never silent: {report}"
    );
    assert!(
        report.contains("FULL pack"),
        "full structural write is stated: {report}"
    );
}

#[test]
fn pack_install_surfaces_advisory_findings_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-advisory");
    let provider = crate::bootstrap::open_provider(&config).unwrap();
    let project_scope = scope_for("proj-advisory");

    // A relationship using a predicate the fixture pack does NOT declare,
    // attached to a graph id the advisory validation can scan.
    let graph_id = engram_domain::KnowledgeGraphId::from("graph-advisory");
    block_on(provider.require_knowledge().unwrap().put_relationship(
        engram_domain::KnowledgeRelationship {
            id: engram_domain::Id::from("rel-undeclared"),
            graph_id: Some(graph_id.clone()),
            subject: engram_domain::EntityRef {
                id: Some(engram_domain::Id::from("Loan")),
                kind: None,
                name: Some("Loan".to_owned()),
                aliases: Vec::new(),
            },
            predicate: "totally_undeclared_predicate".to_owned(),
            code_resolution: None,
            object: engram_domain::EntityRef {
                id: Some(engram_domain::Id::from("Bank")),
                kind: None,
                name: Some("Bank".to_owned()),
                aliases: Vec::new(),
            },
            scope: project_scope.clone(),
            evidence: Vec::new(),
            confidence: None,
            provenance: crate::tools::provenance("pack-test"),
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
        },
    ))
    .unwrap();
    drop(provider);

    let report = pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        Some("graph-advisory"),
    )
    .expect("install completes despite findings");
    assert!(
        report.contains("advisory finding"),
        "findings surface in output: {report}"
    );
    assert!(
        report.contains("non-blocking"),
        "non-blocking mode is stated: {report}"
    );
}

#[test]
fn pack_two_projects_see_only_their_own_pack() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");

    let config_a = mcp_config(&storage, "project-a");
    let config_b = mcp_config(&storage, "project-b");
    pack::install_pack(
        &config_a,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .unwrap();
    pack::install_pack(
        &config_b,
        &load_catalogue(dir.path()),
        "travel-fixture",
        SliceSize::Default,
        None,
    )
    .unwrap();

    let provider = crate::bootstrap::open_provider(&config_a).unwrap();
    let scope_a = scope_for("project-a");
    let scope_b = scope_for("project-b");

    let bank_in_a = block_on(
        provider
            .require_ontology()
            .unwrap()
            .get_ontology(&OntologyId::from("pack-bank-fixture"), &scope_a),
    )
    .unwrap();
    let bank_in_b = block_on(
        provider
            .require_ontology()
            .unwrap()
            .get_ontology(&OntologyId::from("pack-bank-fixture"), &scope_b),
    )
    .unwrap();
    assert!(bank_in_a.is_some(), "project A sees its own pack");
    assert!(bank_in_b.is_none(), "project B does not see A's pack");

    let travel_in_b = block_on(
        provider
            .require_ontology()
            .unwrap()
            .get_ontology(&OntologyId::from("pack-travel-fixture"), &scope_b),
    )
    .unwrap();
    assert!(travel_in_b.is_some(), "project B sees its own pack");

    // Launch configs are per-workspace and separate.
    let a_text = launch_ontology_json(&storage, "project-a");
    let b_text = launch_ontology_json(&storage, "project-b");
    assert!(a_text.contains("bank-fixture"), "A's config names A's pack");
    assert!(
        b_text.contains("travel-fixture"),
        "B's config names B's pack"
    );
    assert!(!a_text.contains("travel-fixture"), "A's config is B-free");
    assert!(!b_text.contains("bank-fixture"), "B's config is A-free");
}

#[test]
fn pack_widen_regenerates_from_cache_offline_and_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-widen");
    pack::install_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::Default,
        None,
    )
    .unwrap();

    // Widen to `all`: regeneration happens from the cache; the URL is never
    // touched (the file:// source is only read on a cache miss, and the cache
    // is populated).
    let widened = pack::widen_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::All,
    )
    .expect("widen from cache");
    assert!(
        widened.contains("zero fetches"),
        "widen states offline regeneration: {widened}"
    );
    assert!(
        widened.contains("restart"),
        "widen prints the restart note (RISK-004): {widened}"
    );
    let widened_text = launch_ontology_json(&storage, "proj-widen");
    assert!(
        widened_text.contains("Mortgage Loan"),
        "widened slice carries terms beyond the default cap order"
    );

    // Deterministic regeneration: a second widen is byte-identical.
    pack::widen_pack(
        &config,
        &load_catalogue(dir.path()),
        "bank-fixture",
        SliceSize::All,
    )
    .unwrap();
    assert_eq!(widened_text, launch_ontology_json(&storage, "proj-widen"));
}

#[test]
fn pack_widen_without_cache_is_an_honest_offline_error() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let config = mcp_config(&storage, "proj-never");
    let err = pack::widen_pack(
        &config,
        &Catalogue::embedded().unwrap(),
        "fibo",
        SliceSize::All,
    )
    .expect_err("widen without cache fails honestly");
    assert!(
        err.contains("offline-only") || err.contains("one-time network"),
        "error names the cache path + one-time network requirement: {err}"
    );
}

/// FIND-code-006 regression: classes sharing a label must never cross-wire
/// hierarchy — the broader pairs are derived from parent_class_ids (unique
/// ids), and the derived taxonomy config resolves a colliding label to the
/// FIRST occurrence's parent.
#[test]
fn pack_label_collisions_wire_hierarchy_by_id() {
    use engram_packs::{ConvertedPack, convert_pack};

    // Two "Shared" classes under different parents, plus the parents.
    let turtle = br#"@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<https://v.test/ParentA> a owl:Class ; rdfs:label "Parent A" .
<https://v.test/ParentB> a owl:Class ; rdfs:label "Parent B" .
<https://v.test/SharedOne> a owl:Class ; rdfs:label "Shared" ; rdfs:subClassOf <https://v.test/ParentA> .
<https://v.test/SharedTwo> a owl:Class ; rdfs:label "Shared" ; rdfs:subClassOf <https://v.test/ParentB> .
"#;
    let converted = convert_pack(
        "collision-fixture",
        "fixture://collision.ttl",
        engram_packs::PackFormat::Turtle,
        turtle,
        "0".repeat(64).as_str(),
        &Scope {
            tenant: "default".to_owned(),
            subject: None,
            workspace: Some("collision".to_owned()),
            session: None,
            environment: None,
        },
        chrono::TimeZone::timestamp_opt(&chrono::Utc, 0, 0).unwrap(),
    )
    .expect("collision fixture converts");

    // Wiring pairs keyed by unique id: each Shared child points at ITS parent.
    let pairs = pack::pack_concept_broader_pairs(&converted);
    let by_child: std::collections::BTreeMap<String, String> = pairs
        .iter()
        .map(|(child, parent)| (child.to_string(), parent.to_string()))
        .collect();
    let id_of = |suffix: &str| {
        converted
            .classes
            .iter()
            .find(|c| c.uri.ends_with(suffix))
            .map(|c| c.id.to_string())
            .unwrap_or_default()
    };
    assert_eq!(
        by_child.get(&id_of("SharedOne")).map(String::as_str),
        Some(id_of("ParentA").as_str()),
        "SharedOne -> ParentA by id, not by the shared label"
    );
    assert_eq!(
        by_child.get(&id_of("SharedTwo")).map(String::as_str),
        Some(id_of("ParentB").as_str()),
        "SharedTwo -> ParentB by id"
    );

    // Derived taxonomy config: the colliding label resolves deterministically
    // to the first occurrence's parent (ParentA — SharedOne sorts first by uri).
    let selection = engram_packs::SliceRule::default().select(
        "collision-fixture",
        &converted.classes,
        &converted.properties,
    );
    let taxonomy = pack::compose_taxonomy_config("collision-fixture", &converted, &selection);
    let shared = taxonomy
        .concepts
        .iter()
        .find(|c| c.label == "Shared")
        .expect("one Shared concept in the config");
    assert_eq!(
        shared.broader.as_deref(),
        Some("Parent A"),
        "first occurrence wins: {:?}",
        taxonomy.concepts
    );
}

/// FIND-code-008 regression: a corrupt cache manifest (arbitrary hash length)
/// produces the corrupt-cache ERROR, never a slicing panic.
#[test]
fn pack_corrupt_manifest_errors_instead_of_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let entry = load_catalogue(dir.path())
        .find("bank-fixture")
        .unwrap()
        .clone();
    // Plant a cache whose manifest carries a truncated hash.
    let cache = pack::cache_dir(&storage, "bank-fixture");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("pack.ttl"), bank_fixture_bytes()).unwrap();
    std::fs::write(
        cache.join("manifest.json"),
        r#"{"id":"bank-fixture","sha256":"deadbeef","license":"CC-BY-4.0","format":"Turtle","url":"x","classes":1,"properties":1}"#,
    )
    .unwrap();
    let err = pack::obtain_bytes(&storage, &entry).expect_err("corrupt manifest must error");
    assert!(
        err.contains("corrupt or off-pin"),
        "named corrupt-cache error: {err}"
    );
    assert!(
        err.contains("delete") && err.contains("refetch"),
        "error names the repair: {err}"
    );
}

/// FIND-code-007 regression: a pinned URL that accepts the connection but
/// never responds fails fast under the fetch timeout instead of wedging the
/// one-shot install.
#[test]
fn pack_fetch_times_out_against_a_hung_url() {
    use std::io::Write as _;
    // A local TCP server that accepts and never responds.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            // Deliberately never write a response.
            let _ = stream.write(&[]);
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().join("store");
    let catalogue = serde_json::json!({
        "_header": {
            "description": "hung-url catalogue", "excluded_language_bases": { "reason": "r" },
            "pin_run": {}, "license_policy": ""
        },
        "entries": [{
            "id": "hung-pack", "name": "Hung", "description": "", "domain": "finance",
            "url": format!("http://127.0.0.1:{port}/pack.ttl"),
            "format": "turtle", "license": "CC-BY-4.0", "license_source": "pack-bytes",
            "sha256": "0".repeat(64)
        }]
    });
    let path = dir.path().join("hung.json");
    std::fs::write(&path, catalogue.to_string()).unwrap();
    let parsed = Catalogue::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let entry = parsed.find("hung-pack").unwrap().clone();

    let guard = std::env::var("ENGRAM_PACK_FETCH_TIMEOUT_SECS").ok();
    // SAFETY: this is the only test in the process that reads this variable
    // (no parallel pack test performs an http fetch), and the write is
    // bracketed by the restore below.
    unsafe { std::env::set_var("ENGRAM_PACK_FETCH_TIMEOUT_SECS", "1") };
    let started = std::time::Instant::now();
    let err = pack::obtain_bytes(&storage, &entry).expect_err("hung URL must time out");
    let elapsed = started.elapsed();
    // SAFETY: see above.
    unsafe {
        match guard {
            Some(v) => std::env::set_var("ENGRAM_PACK_FETCH_TIMEOUT_SECS", v),
            None => std::env::remove_var("ENGRAM_PACK_FETCH_TIMEOUT_SECS"),
        }
    }
    drop(server);
    assert!(err.contains("http"), "error names the http failure: {err}");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "failed fast ({elapsed:?}), not wedged"
    );
}

fn bank_fixture_bytes() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../core/packs/tests/fixtures/bank-fixture.ttl"),
    )
    .unwrap()
}
