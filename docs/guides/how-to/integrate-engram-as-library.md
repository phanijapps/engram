# Integrating Engram as a library

> **Quadrant: how-to + reference.** A complete integration guide for host
> applications that embed Engram as a Rust library through the `EngramProvider`
> SDK facade. Covers every capability area, error handling, and backend
> architecture.

## What Engram provides

Engram is a **backend-neutral memory and knowledge persistence engine**. A host
application opens one `EngramProvider` from a typed config and reaches every
supported capability — memory facts, knowledge graph, provenance/evidence,
atomic batch ingest, unified recall, export/import, and observability — through
backend-neutral Rust traits. The host never touches a database handle, connection,
migration, vector index, or graph table.

**The contract:** swapping the storage backend (e.g., SQLite → a future engine)
is an Engram-internal config/crate change, never an application rewrite. This is
enforced by the ADR-0022 engine-neutrality gate and proven by the S7 stub backend.

## Quick start

### 1. Add Engram as a dependency

```toml
[dependencies]
engram-integration = { path = "..." }   # the SDK facade (provider + ports + DTOs)
engram-conformance = { path = "..." }   # the backend wiring (bootstrap_provider)
engram-domain = { path = "..." }        # domain types (Scope, MemoryRecord, etc.)
engram-runtime = { path = "..." }       # CoreError, CoreResult
```

### 2. Open a provider

```rust,no_run
use engram_integration::{EngramConfig, CapabilityPolicy, MigrationMode,
    EmbeddingProviderConfig, EngramProvider};
use engram_domain::types::ScopeMappingStrategy;

let config = EngramConfig::new(
    "/var/lib/engram",          // storage root
    "/var/lib",                 // trusted root (paths must stay inside)
    ScopeMappingStrategy::Strict,
    EmbeddingProviderConfig {
        provider_type: "fastembed".to_string(),
        model: "bge-small-en-v1.5".to_string(),
        dimensions: 384,
        prompt_profile: "query".to_string(),
        normalization: None,
    },
    MigrationMode::DryRun,
    CapabilityPolicy::FailClosed,
);

// bootstrap_provider constructs the SQLite adapters, runs each family's
// conformance fixture, and attaches a handle only when the fixture passes.
let provider = engram_conformance::wiring::bootstrap_provider(&config)?;
```

### 3. Read the capability report before using a feature

The report has **18 keys**, each `Supported` or a typed `Unsupported { reason }`.

```rust,no_run
use engram_integration::CapabilityState;

let caps = provider.capabilities();
assert!(caps.memory.is_supported(), "memory not wired: {:?}", caps.memory);

// Not-yet-built areas are present and explicit (never silently absent):
match caps.unified_recall {
    CapabilityState::Supported => { /* safe to use */ }
    CapabilityState::Unsupported { reason } => {
        eprintln!("unified_recall unsupported: {reason:?}");
    }
    _ => {}
}
```

## Capability areas

### Memory facts (#4)

Write, retrieve, and forget memory records with scope isolation, confidence,
validity windows, and source metadata.

```rust,no_run
if let Some(memory) = provider.memory() {
    let record = memory.write_memory(request).await?;
    let retrieved = memory.retrieve(query).await?;
    memory.forget(forget_request).await?;
}
```

### Knowledge graph (#5)

Upsert entities and relationships, query neighbors, traverse paths, deduplicate.

```rust,no_run
if let Some(graph) = provider.graph() {
    graph.put_entity(entity).await?;
    graph.put_relationship(relationship).await?;
    let neighbors = graph.neighbors(&entity_id, &scope).await?;
}
```

### Provenance / evidence (#6)

Read the `Provenance` and `EvidenceRef` embedded in any record, and attach new
evidence to an existing record (ADR-0023 port-level rewrite).

```rust,no_run
use engram_domain::EvidenceTargetType;

if let Some(prov = provider.provenance() {
    // Read: what evidence supports this entity?
    let evidence = prov.evidence_for(EvidenceTargetType::Entity, &entity_id, &scope).await?;

    // Write: attach new evidence to an existing record (ADR-0023)
    let updated = prov.attach_evidence(
        EvidenceTargetType::Entity,
        &entity_id,
        new_evidence_ref,
        &scope,
    ).await?;
}
```

**v1 supports Entity, Relationship, Source** for both read and write. Memory,
Belief, Document, Chunk, Concept, Event, Url return `CapabilityUnsupported`.

### Ontology (#7) and Taxonomy (#8)

Register ontology definitions; validate entity/relationship types. Store SKOS-like
concept schemes with broader/narrower/related; expand terms for recall.

```rust,no_run
if let Some(ontology) = provider.ontology() {
    ontology.put_class(class_def).await?;
}
if let Some(taxonomy) = provider.taxonomy() {
    taxonomy.put_concept_scheme(scheme).await?;
}
```

### Belief (#9)

Store beliefs linked to supporting facts; track confidence, validity windows,
and contradictions.

```rust,no_run
if let Some(beliefs) = provider.beliefs() {
    let belief = beliefs.put_belief(belief_record).await?;
    let query = BeliefQuery::live_subject(scope, subject_key, now);
    let found = beliefs.get_belief(query).await?;
}
```

### Atomic batch ingest (#10)

Write a semantic batch (episode + facts + entities + relationships) across stores
in one operation. **Best-effort, not ACID** (separate SQLite files; per-step
partial-failure reporting). Per-record idempotency keys prevent data loss.

```rust,no_run
use engram_integration::{BatchIngestRequest, BatchStep, TransactionGuarantee};

if let Some(batch) = provider.batch() {
    assert_eq!(batch.transaction_guarantee(), TransactionGuarantee::BestEffort);

    let request = BatchIngestRequest {
        idempotency_key: "ingest-2026-07-10".to_string(),
        scope,
        facts: vec![memory_record],
        entities: vec![entity],
        relationships: vec![relationship],
        ..Default::default()
    };
    let outcome = batch.ingest(request).await?;

    // Check per-step results:
    for step in &outcome.steps {
        println!("{}: {:?}", step.step_name(), step.status);
    }
    // Evidence + Embeddings steps are Skipped in v1 (ADR-0024: deferred reindex).
}
```

### Unified recall (#12)

One query that fans across facts (memory), graph, vector, lexical, and beliefs,
fused via Reciprocal Rank Fusion into a `ContextPayload`.

```rust,no_run
if let Some(recall) = provider.recall() {
    let payload = recall.recall(retrieval_request).await?;
    for item in &payload.items {
        println!("{} (score from fusion trace)", item.target_id);
    }
    // Degraded lanes appear in payload.source_failures (not an error).
}
```

**v1 lanes:** facts, graph, lexical, beliefs (wired in production). Vector lane is
feature-gated behind `fastembed` (off by default). Taxonomy expansion + episodes
lanes deferred.

### Export / import (#13, #18)

Export a scope's semantic state into `ImportData`; import via `MigrationService`
(dry-run validation + apply). Round-trip for backend-to-backend movement.

```rust,no_run
if let Some(export_import) = provider.export_import() {
    // Export scope A:
    let data = export_import.export(&scope_a).await?;

    // Import into scope B (via MigrationService):
    let migration = provider.migration().expect("migration handle");
    let report = migration.dry_run_import(&data)?;
    // ... validate report ...
    let manifest = MigrationManifest::from_import_data(&data, &report)?;
    migration.apply_import(&manifest)?;
}
```

**v1 export covers:** knowledge (sources/documents/chunks/entities/relationships),
memory, beliefs, hierarchy, concept schemes/concepts. Vectors deferred.

### Observability (#14)

Query the provider's operational state: capability report, record counts by
semantic type, embedding configuration, schema/adapter versions.

```rust,no_run
if let Some(obs) = provider.observability() {
    let snapshot = obs.diagnostics().await?;
    println!("Entities: {}", snapshot.record_counts.entities);
    println!("Beliefs: {}", snapshot.record_counts.beliefs);
    println!("Embedding: {} ({})", snapshot.embedding_config.model,
             snapshot.embedding_config.dimensions);
}
```

Slow-query/retrieval diagnostics are `None` in v1 (deferred).

## Error handling

Every operation returns `CoreResult<T>` (= `Result<T, CoreError>`). `CoreError`
is a typed enum — discriminate by variant, never by string matching.

```rust,no_run
use engram_runtime::CoreError;

match result {
    Ok(value) => { /* use value */ }
    Err(CoreError::CapabilityUnsupported { capability, reason }) => {
        eprintln!("{} unsupported: {}", capability, reason);
    }
    Err(CoreError::NotFound { target_type, target_id }) => {
        eprintln!("{}:{} not found", target_type, target_id);
    }
    Err(CoreError::ValidationFailed { reason }) => {
        eprintln!("validation: {}", reason);
    }
    Err(CoreError::TransactionUnsupported { capability }) => {
        eprintln!("no transaction for {}", capability);
    }
    Err(CoreError::BackendTransient { backend, message }) => {
        eprintln!("transient {} failure: {}", backend, message);
    }
    Err(e) => { eprintln!("other error: {}", e); }
}
```

**Unsupported capabilities fail explicitly** (typed `CapabilityUnsupported`), never
with a silent fallback. This is the brief's fail-closed principle (`FailClosed`).

## Backend architecture (ADR-0022)

```
   host application
         │
         ▼
   EngramProvider (core/integration)     ← the single SDK entry point
         │
    ┌────┴────┐
    ▼         ▼
  ports    capability report              ← engine-neutral traits + status
    │
    ▼
  adapters/<capability>/<engine>          ← engine cells on a grid
    │
    ▼
  bootstrap_provider (adapters/integration) ← the SQLite recipe
```

**Key principles:**
- The port layer (`core/*`, `core/integration`) names **zero** engine types —
  enforced by `.codex/hooks/check-engine-neutrality.sh`.
- A non-SQLite stub backend (S7) proves the traits are satisfiable without SQLite.
- Adding a future engine (pg, SurrealDB, lance) is an additive adapter cell + a
  new recipe — no application code changes.

## Capability reference (18 areas)

| # | Area | Capability key | v1 state |
|---|---|---|---|
| 1 | Provider facade | (the provider itself) | ✅ Supported |
| 2 | Backend abstraction | (ADR-0022 contract) | ✅ SQLite (proven parametric) |
| 3 | Capability discovery | `CapabilityReport` | ✅ 18 keys, explicit states |
| 4 | Memory facts | `memory` | ✅ Supported |
| 5 | Knowledge graph | `knowledge` + `graph` | ✅ Supported |
| 6 | Episode / evidence | `episodes_evidence` | ✅ Supported (read + write) |
| 7 | Ontology | `ontology` | ✅ Supported |
| 8 | Taxonomy | `taxonomy` | ✅ Supported |
| 9 | Belief | `beliefs` | ✅ Supported |
| 10 | Atomic batch | `atomic_batch` | ✅ Supported (best-effort) |
| 11 | Embedding integration | `vectors` + `embedding_provider` | ✅ Supported (feature-gated) |
| 12 | Unified recall | `unified_recall` | ✅ Supported (vector lane feature-gated) |
| 13 | Maintenance | `migration` | ✅ Supported |
| 14 | Observability | `observability` | ✅ Supported |
| 15 | Stable data model | (domain types) | ✅ Stable IDs, scopes, timestamps |
| 16 | Error model | `CoreError` | ✅ 10 categories, typed |
| 17 | Conformance | (conformance harness) | ✅ Per-family fixtures gate Supported |
| 18 | Migration / export | `export_import` + `migration` | ✅ Supported (vectors deferred) |

## What Engram owns vs. what the host owns

**Engram owns:** backend selection, connection lifecycle, bootstrap, migrations,
schema/index setup, health, maintenance, capability reporting, and every
capability area's storage-neutral contract.

**The host owns:** product behavior — orchestration, UI/API contracts, prompt
policy, recall policy, context budgeting, domain-specific governance. The host
calls the provider's typed handles; it does not manage backend internals.

## Further reading

- [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md) — engine grid vs backend recipe
- [ADR-0023](../../adr/0023-evidence-append-port-level-rewrite.md) — evidence append (port-level rewrite)
- [ADR-0024](../../adr/0024-batch-embeddings-deferred-reindex.md) — batch embeddings (deferred reindex)
- [The brief](../../product/briefs/engram-host-sdk.md) — the host-application requirements decomposition
- `cargo doc -p engram-integration --open` — full API reference
- `examples/rust-integration/` — a working integration example
