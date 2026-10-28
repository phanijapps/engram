//! `engram-mcp pack` one-shot subcommands (plan S7, DEC-P08 shape):
//! `pack list | install | widen | status`.
//!
//! The CLI owns fetch-once-with-sha256-verify, the local cache under
//! `<storage>/packs/cache/`, FULL structural writes through the existing
//! facade ports (`require_ontology` / `require_taxonomy` — zero new MCP
//! tools, D6 intact), the derived launch-config emit per DEC-P04, and the
//! sliced-vs-available reporting per DEC-P05. The pure rules (catalogue,
//! converter, slice) live in `engram-packs`.
//!
//! Network discipline: the only fetch happens on a cache miss, from the
//! pinned URL, verified against the pinned sha256 — a mismatch is a hard
//! error naming the entry and the re-pin procedure (RISK-001). A cache miss
//! while offline is an honest error naming the cache path.

use std::path::{Path, PathBuf};

use engram_domain::{
    Concept, ConceptId, ConceptLabel, ConceptRelation, ConceptRelationKind, ConceptScheme,
    ConceptSchemeId, KnowledgeGraphId, OntologyId, Scope,
};
use engram_packs::{Catalogue, CatalogueEntry, SliceRule, SliceSize};
use futures::executor::block_on;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{bootstrap, config, oneshot, ontology, scope};

/// Entry point for `engram-mcp pack …` argv (after the program name).
pub fn run(argv: &[String]) -> i32 {
    let (cmd, rest) = match argv.split_first() {
        Some((cmd, rest)) => (cmd.as_str(), rest),
        None => {
            eprintln!("engram-mcp pack: needs a subcommand (list | install | widen | status)");
            print_pack_help();
            return 2;
        }
    };
    match cmd {
        "list" => run_list(rest),
        "install" => run_install(rest),
        "widen" => run_widen(rest),
        "status" => run_status(rest),
        "--help" | "-h" | "help" => {
            print_pack_help();
            0
        }
        other => {
            eprintln!(
                "engram-mcp pack: unknown subcommand {other:?} (list | install | widen | status)"
            );
            2
        }
    }
}

fn print_pack_help() {
    println!(
        "usage: engram-mcp pack list [--catalogue <path>]\n\
         \x20      engram-mcp pack install <id> [--slice-size <n>|all] [--catalogue <path>]\n\
         \x20                          [--validate-graph <id>] [--storage <dir>] [--project <name>] ...\n\
         \x20      engram-mcp pack widen <id> [--slice-size <n>|all] [--storage <dir>] [--project <name>] ...\n\
         \x20      engram-mcp pack status [--storage <dir>]\n\
         \n\
         \x20Pack installs are fetch-once (sha256-verified against the pinned catalogue),\n\
         \x20cached under <storage>/packs/cache/, written FULLY into the structural ontology\n\
         \x20and taxonomy repositories, and derived into a bounded launch slice at\n\
         \x20<storage>/packs/launch/<workspace>/ontology.json + taxonomy.json (explicit\n\
         \x20--ontology/--taxonomy launch flags always win)."
    );
}

/// Parsed pack-subcommand flags (the ones the pack surface owns; store/scope
/// flags go straight to `McpConfig::from_args`).
struct PackArgs {
    rest: Vec<String>,
    catalogue_path: Option<String>,
    slice_size: Option<String>,
    validate_graph: Option<String>,
    pack_id: Option<String>,
}

fn parse_pack_args(argv: &[String]) -> Result<PackArgs, String> {
    let mut args = PackArgs {
        rest: Vec::new(),
        catalogue_path: None,
        slice_size: None,
        validate_graph: None,
        pack_id: None,
    };
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--catalogue" => {
                args.catalogue_path = Some(it.next().ok_or("--catalogue needs a path")?.clone())
            }
            "--slice-size" => {
                args.slice_size = Some(it.next().ok_or("--slice-size needs <n>|all")?.clone())
            }
            "--validate-graph" => {
                args.validate_graph = Some(
                    it.next()
                        .ok_or("--validate-graph needs a graph id")?
                        .clone(),
                )
            }
            "--help" | "-h" => {
                print_pack_help();
                std::process::exit(0);
            }
            other if !other.starts_with("--") && args.pack_id.is_none() => {
                args.pack_id = Some(other.to_owned());
            }
            other => args.rest.push(other.to_owned()),
        }
    }
    Ok(args)
}

fn load_catalogue(path: Option<&str>) -> Result<Catalogue, String> {
    match path {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .map_err(|e| format!("read catalogue override {p}: {e}"))?;
            Catalogue::from_json(&text)
        }
        None => Catalogue::embedded(),
    }
}

// ---------------------------------------------------------------------------
// pack list
// ---------------------------------------------------------------------------

fn run_list(argv: &[String]) -> i32 {
    let args = match parse_pack_args(argv) {
        Ok(a) => a,
        Err(message) => {
            eprintln!("engram-mcp pack list: {message}");
            return 2;
        }
    };
    let catalogue = match load_catalogue(args.catalogue_path.as_deref()) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack list: {message}");
            return 1;
        }
    };
    println!("{} pinned pack(s):", catalogue.entries.len());
    for entry in &catalogue.entries {
        println!(
            "{:<14} {:<18} {:<24} {}",
            entry.id, entry.domain, entry.license, entry.name
        );
    }
    0
}

// ---------------------------------------------------------------------------
// shared plumbing: cache, fetch, verify
// ---------------------------------------------------------------------------

/// The per-storage pack cache root: `<storage>/packs/cache/`.
pub(crate) fn cache_root(storage: &Path) -> PathBuf {
    storage.join("packs").join("cache")
}

pub(crate) fn cache_dir(storage: &Path, pack_id: &str) -> PathBuf {
    cache_root(storage).join(pack_id)
}

/// Immutable cache manifest — identity + counts, no timestamps (determinism).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct CacheManifest {
    id: String,
    sha256: String,
    license: String,
    format: String,
    url: String,
    classes: u64,
    properties: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchSource {
    Cache,
    Fetch,
}

/// Cache-or-fetch the pinned bytes with sha256 verification on BOTH paths.
///
/// A cached entry whose bytes no longer match its manifest is treated as
/// corrupt and refused (delete-and-refetch is the documented repair). A
/// fetched entry that mismatches the pinned sha256 is a hard error naming
/// the entry and the re-pin procedure (RISK-001) — divergent bytes are
/// never installed and the pin is never overwritten.
fn obtain_bytes(storage: &Path, entry: &CatalogueEntry) -> Result<(Vec<u8>, FetchSource), String> {
    let dir = cache_dir(storage, &entry.id);
    let pack_path = dir.join(format!("pack.{}", entry.format.extension()));
    let manifest_path = dir.join("manifest.json");
    if pack_path.exists() && manifest_path.exists() {
        let bytes = std::fs::read(&pack_path)
            .map_err(|e| format!("read cached pack {}: {e}", pack_path.display()))?;
        let manifest: CacheManifest = serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("read cache manifest: {e}"))?,
        )
        .map_err(|e| format!("parse cache manifest: {e}"))?;
        let actual = sha256_hex(&bytes);
        if actual != manifest.sha256 || manifest.sha256 != entry.sha256 {
            return Err(format!(
                "pack cache for {} is corrupt or off-pin (manifest {}, catalog {}, bytes {}): \
                 delete {} and re-run to refetch from the pinned URL",
                entry.id,
                &manifest.sha256[..12],
                &entry.sha256[..12],
                &actual[..12],
                dir.display()
            ));
        }
        return Ok((bytes, FetchSource::Cache));
    }

    // Cache miss: fetch once from the pinned URL and hard-verify.
    let bytes = fetch_url(&entry.url)
        .map_err(|e| format!("fetch pack {} from {}: {e}", entry.id, entry.url))?;
    let actual = sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(format!(
            "sha256 mismatch at first fetch for pack {}: pinned {}, fetched {} — refusing to \
             install divergent bytes. The pinned URL served different content than the pin run \
             (RISK-001). Re-pin procedure: record the new upstream bytes deliberately (update \
             core/packs/data/catalogue.json url+sha256 in a reviewed change); never edit the \
             pin to make a fetch pass silently.",
            entry.id,
            &entry.sha256[..12],
            &actual[..12]
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create cache dir: {e}"))?;
    std::fs::write(&pack_path, &bytes).map_err(|e| format!("write cache: {e}"))?;
    Ok((bytes, FetchSource::Fetch))
}

/// Fetch pinned bytes: `file://` reads a local file (the dev/fixture
/// mechanism — deterministic, offline), `http(s)://` uses the blocking
/// reqwest client with the rustls transport.
fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| format!("read {path}: {e}"));
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("unsupported URL scheme: {url}"));
    }
    let response = reqwest::blocking::get(url).map_err(|e| format!("http: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("http status {}", response.status()));
    }
    let bytes = response.bytes().map_err(|e| format!("read body: {e}"))?;
    Ok(bytes.to_vec())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ---------------------------------------------------------------------------
// pack install
// ---------------------------------------------------------------------------

fn run_install(argv: &[String]) -> i32 {
    let args = match parse_pack_args(argv) {
        Ok(a) => a,
        Err(message) => {
            eprintln!("engram-mcp pack install: {message}");
            return 2;
        }
    };
    let Some(pack_id) = args.pack_id.clone() else {
        eprintln!("engram-mcp pack install: needs a pack id (see `pack list`)");
        return 2;
    };
    let slice_size = match SliceSize::parse(args.slice_size.as_deref()) {
        Ok(s) => s,
        Err(message) => {
            eprintln!("engram-mcp pack install: {message}");
            return 2;
        }
    };
    let catalogue = match load_catalogue(args.catalogue_path.as_deref()) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack install: {message}");
            return 1;
        }
    };
    let mcp_config = match config::McpConfig::from_args(&args.rest) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack install: {message}");
            return 2;
        }
    };

    match install_pack(
        &mcp_config,
        &catalogue,
        &pack_id,
        slice_size,
        args.validate_graph.as_deref(),
    ) {
        Ok(report) => {
            println!("{report}");
            0
        }
        Err(message) => {
            eprintln!("engram-mcp pack install: {message}");
            1
        }
    }
}

/// Install one pinned pack into the configured project store.
pub(crate) fn install_pack(
    mcp_config: &config::McpConfig,
    catalogue: &Catalogue,
    entry_id: &str,
    slice_size: SliceSize,
    validate_graph: Option<&str>,
) -> Result<String, String> {
    let entry = catalogue
        .find(entry_id)
        .ok_or_else(|| format!("unknown pack id {entry_id:?} (see `pack list`)"))?;
    let provider = bootstrap::open_provider(mcp_config)?;
    let workspace_scope = scope::resolve_scope(
        mcp_config.org.as_deref(),
        mcp_config.domain.as_deref(),
        mcp_config.subdomain.as_deref(),
        &mcp_config.project,
    );
    let storage = &mcp_config.storage_path;

    let (bytes, source) = obtain_bytes(storage, entry)?;
    let fetched_line = match source {
        FetchSource::Cache => "source: cache (zero fetches)".to_owned(),
        FetchSource::Fetch => {
            format!("source: fetch (sha256 verified: {}…)", &entry.sha256[..12])
        }
    };

    let converted = engram_packs::convert_pack(
        &entry.id,
        &entry.url,
        entry.format,
        &bytes,
        &entry.sha256,
        &workspace_scope,
        chrono::Utc::now(),
    )?;

    // FULL structural writes through the existing facade ports (DEC-008).
    let ontology_port = provider
        .require_ontology()
        .map_err(|e| format!("ontology capability unavailable: {e}"))?;
    block_on(ontology_port.put_ontology(converted.ontology.clone()))
        .map_err(|e| format!("put_ontology: {e}"))?;
    for class in &converted.classes {
        block_on(ontology_port.put_class(class.clone()))
            .map_err(|e| format!("put_class {}: {e}", class.label))?;
    }
    for property in &converted.properties {
        block_on(ontology_port.put_property(property.clone()))
            .map_err(|e| format!("put_property {}: {e}", property.label))?;
    }

    // Taxonomy mirror: one concept scheme per pack + one concept per class,
    // broader relations carrying the hierarchy — readable through the
    // existing TaxonomyRepository port (structural read-back with counts).
    let taxonomy_port = provider
        .require_taxonomy()
        .map_err(|e| format!("taxonomy capability unavailable: {e}"))?;
    let scheme_id = ConceptSchemeId::from(format!("pack-{}", entry.id));
    let now = chrono::Utc::now();
    block_on(taxonomy_port.put_concept_scheme(ConceptScheme {
        id: scheme_id.clone(),
        uri: entry.url.clone(),
        name: format!("pack:{}", entry.id),
        scope: workspace_scope.clone(),
        version: format!("pinned:{}", &entry.sha256[..12]),
        provenance: converted.ontology.provenance.clone(),
        policy: converted.ontology.policy.clone(),
        created_at: now,
        updated_at: None,
    }))
    .map_err(|e| format!("put_concept_scheme: {e}"))?;
    // FIND-code-006: concepts and their broader wiring are keyed by the
    // unique class id (labels are display-only — classes can share a label).
    let concept_id =
        |class_id: &engram_domain::OntologyClassId| ConceptId::from(class_id.to_string());
    for class in &converted.classes {
        block_on(taxonomy_port.put_concept(Concept {
            id: concept_id(&class.id),
            uri: class.uri.clone(),
            scheme_id: scheme_id.clone(),
            pref_label: ConceptLabel {
                value: class.label.clone(),
                language: None,
            },
            alt_labels: Vec::new(),
            definition: class.description.clone(),
            notation: None,
            status: engram_domain::ConceptStatus::Active,
            provenance: class.provenance.clone(),
            created_at: now,
            updated_at: None,
        }))
        .map_err(|e| format!("put_concept {}: {e}", class.label))?;
    }
    for (child, parent) in pack_concept_broader_pairs(&converted) {
        block_on(taxonomy_port.put_concept_relation(ConceptRelation {
            id: format!("{}>{}", child, parent),
            scheme_id: scheme_id.clone(),
            subject_id: child,
            predicate: ConceptRelationKind::Broader,
            object_id: parent,
            provenance: converted.ontology.provenance.clone(),
            created_at: now,
        }))
        .map_err(|e| format!("put_concept_relation: {e}"))?;
    }

    // Derived launch slice (DEC-P05) + config emit (DEC-P04).
    let rule = SliceRule::from_size(slice_size);
    let selection = rule.select(&entry.id, &converted.classes, &converted.properties);
    write_launch_configs(storage, &workspace_scope, entry, &converted, &selection)?;

    // Advisory validation (REQ-014): findings surface, install completes.
    let findings_line = match validate_graph {
        Some(graph_id) => {
            let findings = block_on(ontology_port.validate_graph(
                &KnowledgeGraphId::from(graph_id.to_owned()),
                &OntologyId::from(format!("pack-{}", entry.id)),
                &workspace_scope,
            ))
            .unwrap_or_default();
            if findings.is_empty() {
                "validation: clean (0 findings)".to_owned()
            } else {
                format!(
                    "validation: {} advisory finding(s), non-blocking — first: {}",
                    findings.len(),
                    findings[0].message
                )
            }
        }
        None => "validation: skipped (pass --validate-graph <id> to check a graph)".to_owned(),
    };

    // Manifest update with available counts for `pack status`.
    let manifest = CacheManifest {
        id: entry.id.clone(),
        sha256: entry.sha256.clone(),
        license: entry.license.clone(),
        format: format!("{:?}", entry.format),
        url: entry.url.clone(),
        classes: converted.classes.len() as u64,
        properties: converted.properties.len() as u64,
    };
    let dir = cache_dir(storage, &entry.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create cache dir: {e}"))?;
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&manifest).map_err(|e| format!("serialize manifest: {e}"))?,
    )
    .map_err(|e| format!("write manifest: {e}"))?;

    Ok(format!(
        "pack install {} — {fetched_line}\n  structural: 1 ontology, {} classes, {} properties, taxonomy mirror {} concepts (FULL pack)\n  launch: {} + {} (default slice: {})\n  {findings_line}\n  note: launch config is read at server start — restart the server to activate this pack",
        entry.id,
        converted.classes.len(),
        converted.properties.len(),
        converted.classes.len(),
        launch_dir(storage, &workspace_scope)
            .join("ontology.json")
            .display(),
        launch_dir(storage, &workspace_scope)
            .join("taxonomy.json")
            .display(),
        selection.counts_line(),
    ))
}

/// `<storage>/packs/launch/<workspace>/` — the conventional launch path
/// (DEC-P03); loaded by the server only when no explicit flag is passed.
pub fn launch_dir(storage: &Path, workspace_scope: &Scope) -> PathBuf {
    let workspace = workspace_scope
        .workspace
        .clone()
        .unwrap_or_else(|| "default".to_owned());
    storage.join("packs").join("launch").join(workspace)
}

/// Derive + write the launch configs (deterministic, no timestamps).
fn write_launch_configs(
    storage: &Path,
    workspace_scope: &Scope,
    entry: &CatalogueEntry,
    converted: &engram_packs::ConvertedPack,
    selection: &engram_packs::SliceSelection,
) -> Result<(), String> {
    let ontology_config = compose_ontology_config(&entry.id, selection);
    let taxonomy_config = compose_taxonomy_config(&entry.id, converted, selection);
    let dir = launch_dir(storage, workspace_scope);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create launch dir: {e}"))?;
    let ontology_json =
        serde_json::to_string_pretty(&ontology_config).map_err(|e| format!("serialize: {e}"))?;
    let taxonomy_json =
        serde_json::to_string_pretty(&taxonomy_config).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(dir.join("ontology.json"), ontology_json + "\n")
        .map_err(|e| format!("write ontology.json: {e}"))?;
    std::fs::write(dir.join("taxonomy.json"), taxonomy_json + "\n")
        .map_err(|e| format!("write taxonomy.json: {e}"))?;
    Ok(())
}

/// DEC-P04: default 3-layer config PLUS one `<pack-id>` layer carrying the
/// sliced classes; `within` = default within ∪ sliced pack predicates;
/// `across` unchanged.
pub fn compose_ontology_config(
    pack_id: &str,
    selection: &engram_packs::SliceSelection,
) -> ontology::OntologyConfig {
    let mut cfg = ontology::OntologyConfig::default();
    cfg.layers.push(ontology::OntologyLayer {
        name: pack_id.to_owned(),
        classes: selection.class_labels.clone(),
    });
    let mut within = cfg.predicates.within.clone();
    for predicate in &selection.predicate_labels {
        if !within.contains(predicate) {
            within.push(predicate.clone());
        }
    }
    cfg.predicates.within = within;
    cfg
}

/// Derived taxonomy: one scheme named for the pack, sliced class labels as
/// concepts, `broader` set when the parent class is also in the slice.
pub fn compose_taxonomy_config(
    pack_id: &str,
    converted: &engram_packs::ConvertedPack,
    selection: &engram_packs::SliceSelection,
) -> ontology::TaxonomyConfig {
    // FIND-code-006: hierarchy lookup keyed by the unique class ID; a label
    // collision resolves to the FIRST occurrence (the same class the slice's
    // dedupe kept), so a shared label never attaches another class's parent.
    let id_to_label: std::collections::BTreeMap<String, String> = converted
        .classes
        .iter()
        .map(|c| (c.id.to_string(), c.label.clone()))
        .collect();
    let mut label_to_parent: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for class in &converted.classes {
        let Some(parent_id) = class.parent_class_ids.first() else {
            continue;
        };
        let Some(parent_label) = id_to_label.get(parent_id.as_str()) else {
            continue;
        };
        label_to_parent
            .entry(class.label.clone())
            .or_insert(parent_label.clone());
    }
    let concepts = selection
        .class_labels
        .iter()
        .map(|label| ontology::TaxonomyConcept {
            label: label.clone(),
            broader: label_to_parent
                .get(label)
                .filter(|parent| selection.class_labels.contains(parent))
                .cloned(),
        })
        .collect();
    ontology::TaxonomyConfig {
        name: pack_id.to_owned(),
        concepts,
    }
}

/// Broader (child → parent) concept-id pairs for the taxonomy mirror —
/// derived from `parent_class_ids` (unique ids), never from labels, so
/// colliding labels cannot cross-wire hierarchy edges (FIND-code-006).
pub(crate) fn pack_concept_broader_pairs(
    converted: &engram_packs::ConvertedPack,
) -> Vec<(ConceptId, ConceptId)> {
    let known: std::collections::BTreeSet<String> =
        converted.classes.iter().map(|c| c.id.to_string()).collect();
    converted
        .classes
        .iter()
        .flat_map(|class| {
            class.parent_class_ids.iter().filter_map(|parent_id| {
                known.contains(parent_id.as_str()).then(|| {
                    (
                        ConceptId::from(class.id.to_string()),
                        ConceptId::from(parent_id.to_string()),
                    )
                })
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// pack widen
// ---------------------------------------------------------------------------

fn run_widen(argv: &[String]) -> i32 {
    let args = match parse_pack_args(argv) {
        Ok(a) => a,
        Err(message) => {
            eprintln!("engram-mcp pack widen: {message}");
            return 2;
        }
    };
    let Some(pack_id) = args.pack_id.clone() else {
        eprintln!("engram-mcp pack widen: needs a pack id");
        return 2;
    };
    let slice_size = match SliceSize::parse(args.slice_size.as_deref()) {
        Ok(s) => s,
        Err(message) => {
            eprintln!("engram-mcp pack widen: {message}");
            return 2;
        }
    };
    let mcp_config = match config::McpConfig::from_args(&args.rest) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack widen: {message}");
            return 2;
        }
    };
    let catalogue = match load_catalogue(args.catalogue_path.as_deref()) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack widen: {message}");
            return 1;
        }
    };
    match widen_pack(&mcp_config, &catalogue, &pack_id, slice_size) {
        Ok(report) => {
            println!("{report}");
            0
        }
        Err(message) => {
            eprintln!("engram-mcp pack widen: {message}");
            1
        }
    }
}

/// Widen = regenerate the launch config from the CACHE under a larger rule.
/// Offline by construction: no URL is ever touched (REQ-007).
pub(crate) fn widen_pack(
    mcp_config: &config::McpConfig,
    catalogue: &Catalogue,
    pack_id: &str,
    slice_size: SliceSize,
) -> Result<String, String> {
    let entry = catalogue
        .find(pack_id)
        .ok_or_else(|| format!("unknown pack id {pack_id:?} (see `pack list`)"))?;
    let storage = &mcp_config.storage_path;
    let dir = cache_dir(storage, pack_id);
    let pack_path = dir.join(format!("pack.{}", entry.format.extension()));
    if !pack_path.exists() {
        return Err(format!(
            "pack {pack_id} is not cached ({} missing) — widening is offline-only; run \
             `pack install {pack_id}` once (one-time network) to populate the cache",
            pack_path.display()
        ));
    }
    let bytes = std::fs::read(&pack_path).map_err(|e| format!("read cache: {e}"))?;
    let actual = sha256_hex(&bytes);
    if actual != entry.sha256 {
        return Err(format!(
            "cached bytes for {pack_id} are off-pin ({}) — delete {} and re-install",
            &actual[..12],
            dir.display()
        ));
    }
    let workspace_scope = scope::resolve_scope(
        mcp_config.org.as_deref(),
        mcp_config.domain.as_deref(),
        mcp_config.subdomain.as_deref(),
        &mcp_config.project,
    );
    let converted = engram_packs::convert_pack(
        &entry.id,
        &entry.url,
        entry.format,
        &bytes,
        &entry.sha256,
        &workspace_scope,
        chrono::Utc::now(),
    )?;
    let rule = SliceRule::from_size(slice_size);
    let selection = rule.select(&entry.id, &converted.classes, &converted.properties);
    write_launch_configs(storage, &workspace_scope, entry, &converted, &selection)?;
    Ok(format!(
        "pack widen {pack_id} — regenerated from cache (zero fetches): {}\n  launch: {}\n  note: config is read at server start — restart the server to pick up the widened slice (RISK-004)",
        selection.counts_line(),
        launch_dir(storage, &workspace_scope).display()
    ))
}

// ---------------------------------------------------------------------------
// pack status
// ---------------------------------------------------------------------------

fn run_status(argv: &[String]) -> i32 {
    let args = match parse_pack_args(argv) {
        Ok(a) => a,
        Err(message) => {
            eprintln!("engram-mcp pack status: {message}");
            return 2;
        }
    };
    let mcp_config = match config::McpConfig::from_args(&args.rest) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("engram-mcp pack status: {message}");
            return 2;
        }
    };
    let storage = &mcp_config.storage_path;
    let mut lines = Vec::new();

    // Cache entries with hashes + available counts.
    let cache = cache_root(storage);
    if cache.exists() {
        let mut ids: Vec<String> = std::fs::read_dir(&cache)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        lines.push(format!("cache ({} entries):", ids.len()));
        for id in ids {
            let dir = cache.join(&id);
            let pack_path = dir.join("pack.ttl");
            let pack_path_alt = dir.join("pack.rdf");
            let path = if pack_path.exists() {
                pack_path
            } else {
                pack_path_alt
            };
            let bytes = std::fs::read(&path).unwrap_or_default();
            let manifest: Option<CacheManifest> =
                std::fs::read_to_string(dir.join("manifest.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
            match manifest {
                Some(m) => lines.push(format!(
                    "  {} sha256 {}… license {} ({} classes / {} properties available)",
                    id,
                    &m.sha256[..12],
                    m.license,
                    m.classes,
                    m.properties
                )),
                None => lines.push(format!(
                    "  {} sha256 {}… (no manifest)",
                    id,
                    &sha256_hex(&bytes).get(..12).unwrap_or("?")
                )),
            }
        }
    } else {
        lines.push("cache: empty (no packs installed for this storage)".to_owned());
    }

    // Launch configs per workspace with file hashes + sliced counts.
    let launch_root = storage.join("packs").join("launch");
    if launch_root.exists() {
        let mut workspaces: Vec<String> = std::fs::read_dir(&launch_root)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        workspaces.sort();
        for workspace in workspaces {
            let dir = launch_root.join(&workspace);
            let ontology_path = dir.join("ontology.json");
            if !ontology_path.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&ontology_path).unwrap_or_default();
            let parsed = ontology::OntologyConfig::from_json(&text).ok();
            let pack_layers: Vec<String> = parsed
                .as_ref()
                .map(|cfg| {
                    cfg.layers
                        .iter()
                        .filter(|l| !["technical", "domain", "business"].contains(&l.name.as_str()))
                        .map(|l| l.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            let counts = parsed
                .as_ref()
                .map(|cfg| {
                    let pack_classes: usize = cfg
                        .layers
                        .iter()
                        .filter(|l| !["technical", "domain", "business"].contains(&l.name.as_str()))
                        .map(|l| l.classes.len())
                        .sum();
                    format!(
                        "{pack_classes} sliced classes, {} within predicates",
                        cfg.predicates.within.len()
                    )
                })
                .unwrap_or_else(|| "unparseable".to_owned());
            lines.push(format!(
                "workspace {workspace}: launch config {} (pack layer(s): {}, {counts})",
                &sha256_hex(text.as_bytes()).get(..12).unwrap_or("?"),
                pack_layers.join(", ")
            ));
        }
    }
    println!("{}", lines.join("\n"));
    0
}

/// JSON snapshot for scripted consumers (used by the catalogue check loop).
#[allow(dead_code)]
fn status_json(storage: &Path) -> serde_json::Value {
    let cache = cache_root(storage);
    let entries: Vec<serde_json::Value> = std::fs::read_dir(&cache)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| {
                    let id = e.file_name().into_string().ok()?;
                    let manifest: CacheManifest =
                        std::fs::read_to_string(e.path().join("manifest.json"))
                            .ok()
                            .and_then(|t| serde_json::from_str(&t).ok())?;
                    Some(json!({
                        "id": id,
                        "sha256": manifest.sha256,
                        "license": manifest.license,
                        "classes": manifest.classes,
                        "properties": manifest.properties,
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "cache": entries })
}

/// Re-exported for the one-shot dispatcher: is argv `pack …`?
pub fn is_pack_command(argv: &[String]) -> bool {
    argv.first().map(String::as_str) == Some("pack") && argv.len() > 1
}

#[allow(unused_imports)]
use oneshot as _oneshot;
