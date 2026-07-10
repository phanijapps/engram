# How to use Engram as a memory and knowledge library

> **Quadrant: how-to.** A focused walkthrough for a host application that embeds
> Engram as a Rust library through the `EngramProvider` SDK facade.

## Goal

Open one Engram provider from a typed config, read its capability report, and
call a supported service — without ever touching a backend-specific database
handle, connection, migration, vector index, or graph table.

## You should already have

- The `engram-integration` crate (the SDK facade) and the `engram-conformance`
  crate (the backend wiring) in your workspace.
- A Rust 2021+ toolchain.

## Open the provider

`EngramProvider` is the single entry point. Construct an `EngramConfig` and
bootstrap through the backend wiring layer — never call a backend adapter
directly.

```rust,no_run
use engram_integration::{EngramConfig, CapabilityPolicy, MigrationMode,
    EmbeddingProviderConfig, EngramProvider};
use engram_domain::types::ScopeMappingStrategy;

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

// The backend wiring layer (engram-conformance::wiring::bootstrap_provider)
// constructs the SQLite adapters, runs each family's conformance fixture, and
// attaches a handle only when the fixture passes.
let provider = engram_conformance::wiring::bootstrap_provider(&config)?;
```

`EngramProvider::bootstrap(&config)` validates config only — it returns a
provider with every family `Unsupported` and no handles. Use the wiring-layer
bootstrap to get a provider with real, fixture-verified handles.

## Read the capability report before you use a feature

The report has **18 keys**, one per capability area. Each is `Supported`, or a
typed non-`Supported` state carrying a stable reason code. Check before you call;

never assume:

```rust,no_run
use engram_integration::CapabilityState;

let caps = provider.capabilities();
assert!(caps.memory.is_supported(), "memory not supported: {:?}", caps.memory);

// The 8 not-yet-built areas are present and explicit:
match caps.episodes_evidence {
    CapabilityState::Unsupported { .. } => { /* episodes/evidence slice not shipped yet */ }
    _ => { /* safe to use */ }
}
```

Unsupported operations fail with typed `CoreError` variants across the brief's
10 categories (unsupported capability, backend unavailable, migration
required/failed, embedding mismatch, validation failed, conflict, transaction
unsupported, transient/permanent backend failure) — discriminate by variant,
never by string match.

## Call a supported family

Each family is a backend-neutral `Arc<dyn ...>` handle, `Some` only when
supported:

```rust,no_run
if let Some(memory) = provider.memory() {
    // memory: Arc<dyn MemoryService> — write, retrieve, forget.
}
if let Some(graph) = provider.graph() {
    // graph: Arc<dyn KnowledgeGraphRepository> — entities, relationships, neighbors.
}
```

## What Engram owns vs. what you own

- **Engram owns** backend selection, connection lifecycle, bootstrap,
  migrations, schema/index setup, health, maintenance, and capability reporting.
- **You own** product behavior: orchestration, UI/API contracts, prompt policy,
  recall policy, context budgeting, domain governance.

## Backends

SQLite is the active backend today. The engine-neutral contract (ADR-0022) means
swapping to a future backend is an Engram-internal config/crate change — your
application code does not change. Selecting a backend is declarative in config;
the same high-level APIs work across supported backends.

## Further reading

- `engram-host-sdk` brief — `docs/product/briefs/engram-host-sdk.md`
- ADR-0022 — engine grid vs backend recipe
- `rust-crate-integration` spec — the provider facade contract
- API: `cargo doc -p engram-integration --open`
