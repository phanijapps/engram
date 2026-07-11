# How to use Engram as a memory and knowledge library

> **Quadrant: how-to.** A focused walkthrough for a host application that embeds
> Engram as a Rust library through the `EngramProvider` SDK facade.

## Goal

Open one Engram provider from a typed config, read its capability report, and
call a supported service — without ever touching a backend-specific database
handle, connection, migration, vector index, or graph table.

## You should already have

- The `engram-integration` crate in your workspace. That is your **only**
  engram dependency — no `engram-conformance`, no adapter crates.
- A Rust toolchain (edition 2024).

```toml
[dependencies]
engram-integration = { version = "…", features = ["sqlite"] }
```

## Open the provider

`EngramProvider::open(config)` is the sole entry point. It owns backend
selection, connection lifecycle, bootstrap, migration checks, and capability
evaluation — all behind the `sqlite` feature flag.

### Option A: programmatic config

```rust,no_run
use engram_integration::{EngramConfig, CapabilityPolicy, MigrationMode,
    EmbeddingProviderConfig, EngramProvider, ScopeMappingStrategy};

let config = EngramConfig::new(
    "/var/lib/engram",          // storage root
    "/var/lib",                 // trusted root (paths must stay inside)
    ScopeMappingStrategy::Strict,
    EmbeddingProviderConfig {
        provider_type: "test".to_string(),
        model: "test_model".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    },
    MigrationMode::DryRun,
    CapabilityPolicy::FailClosed,
);

let provider = EngramProvider::open(&config)?;
```

### Option B: config file (backend profile)

```toml
# semantic-engine.toml
[backend]
kind = "sqlite"
data_root = "/var/lib/engram"
```

```rust,no_run
use engram_integration::{EngramConfig, EngramProvider};

let config = EngramConfig::from_profile_file("semantic-engine.toml")?;
let provider = EngramProvider::open(&config)?;
```

To switch backends, change the TOML — no code changes:

```toml
# production profile — change this file, not your code
[backend]
kind = "postgres"
connection_string = "postgres://…"
```

(Postgres/Surreal return `CapabilityUnsupported` until those backends ship; the
profile enum reserves the variants.)

## Require a handle (typed errors)

Every family has a `require_*` method that returns a typed
`CoreError::CapabilityUnsupported` when the handle is absent — no silent
fallback, no `Option` unwrapping:

```rust,no_run
// Returns &Arc<dyn MemoryService> or a typed error
let memory = provider.require_memory()?;

// The error is machine-actionable:
match provider.require_recall() {
    Ok(recall) => recall.recall(request).await?,
    Err(engram_runtime::CoreError::CapabilityUnsupported { capability, reason }) => {
        eprintln!("{} unsupported: {}", capability, reason);
    }
    Err(e) => return Err(e.into()),
}
```

### All require_* handles

| Method | Trait | What it does |
|---|---|---|
| `require_memory()` | `MemoryService` | Write, retrieve, forget memory records |
| `require_knowledge()` | `KnowledgeRepository` | Sources, documents, chunks |
| `require_graph()` | `KnowledgeGraphRepository` | Entities, relationships, neighbors |
| `require_ontology()` | `OntologyRepository` | Class/property/axiom definitions |
| `require_taxonomy()` | `TaxonomyRepository` | Concept schemes, concepts |
| `require_beliefs()` | `BeliefRepository` | Beliefs, contradictions |
| `require_hierarchy()` | `HierarchyRepository` | Hierarchy nodes, paths |
| `require_provenance()` | `ProvenanceQuery` | Read/attach evidence + provenance |
| `require_batch()` | `BatchIngest` | Best-effort semantic batch ingest |
| `require_recall()` | `UnifiedRecall` | Fused multi-lane recall |
| `require_export_import()` | `ExportImport` | Export scope state |
| `require_observability()` | `Observability` | Diagnostics snapshot |
| `require_migration()` | `MigrationService` | Dry-run + apply imports |

## Read the capability report

The report has **18 keys**, each `Supported` or a typed `Unsupported { reason }`.
Use it at startup to gate features:

```rust,no_run
use engram_integration::CapabilityState;

let caps = provider.capabilities();
if !caps.memory.is_supported() {
    return Err("memory backend not wired");
}
```

## Error handling

Every operation returns `CoreResult<T>` (= `Result<T, CoreError>`). Discriminate
by variant, never by string matching:

```rust,no_run
use engram_runtime::CoreError;

match result {
    Ok(value) => { /* use value */ }
    Err(CoreError::CapabilityUnsupported { capability, reason }) => { /* typed */ }
    Err(CoreError::NotFound { target_type, target_id }) => { /* typed */ }
    Err(CoreError::ValidationFailed { reason }) => { /* typed */ }
    Err(CoreError::TransactionUnsupported { capability }) => { /* typed */ }
    Err(CoreError::BackendTransient { backend, message }) => { /* retry */ }
    Err(e) => { eprintln!("other: {}", e); }
}
```

## What Engram owns vs. what you own

- **Engram owns** backend selection, connection lifecycle, bootstrap,
  migrations, schema/index setup, health, maintenance, and capability reporting.
- **You own** product behavior: orchestration, UI/API contracts, prompt policy,
  recall policy, context budgeting, domain governance.

Your sidecar schema may store workflow state and opaque Engram IDs, but **not**
semantic records, embeddings, graph edges, or backend query logic.

## Backends

SQLite is the only implemented backend today. The engine-neutral contract
(ADR-0022) means swapping to a future backend (Postgres, Surreal) is a
config-file change, not a code change. The neutrality gate
(`check-engine-neutrality.sh`) enforces that the port layer names zero engine
types. A stub backend (S7) proves the traits are satisfiable without SQLite.

## Further reading

- [Integrating Engram as a library](integrate-engram-as-library.md) — full reference
- ADR-0022 — engine grid vs backend recipe
- ADR-0023 — evidence append (port-level rewrite)
- ADR-0024 — batch embeddings (deferred reindex)
- `engram-host-sdk` brief — `docs/product/briefs/engram-host-sdk.md`
- API: `cargo doc -p engram-integration --open`
