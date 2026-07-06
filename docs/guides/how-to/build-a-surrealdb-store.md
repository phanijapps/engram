# Use SurrealDB as the Engram store

Swap Engram's SQLite adapters for a SurrealDB-backed store you write. At the end you'll have a SurrealDB adapter crate that implements the same port traits the SQLite adapter does, wired into the provider so a host can boot Engram against SurrealDB instead of five SQLite files.

This guide is for a Rust developer who has Engram integrated and wants a different backend. If you're new to Engram, start with [Add memory to a Rust agent with Engram](../tutorials/use-engram-as-memory-layer.md) and come back.

> The SurrealDB-specific code below — the `surrealdb` client calls and SURQL — is **illustrative**. Engram ships no SurrealDB adapter, so it cannot be run verbatim here. The contract steps (which traits to implement, the crate layout, workspace registration, wiring, and verification) document the real, shipped surface and are exact. Adapt the SurrealDB snippets to the `surrealdb` crate version you depend on.

## How Engram separates storage from behavior

Engram never asks an adapter to own domain logic. Domain types and invariants live in `engram-domain`; behavior ports (the traits your adapter implements) live in the `engram-*` core crates; storage adapters live behind those ports. Your SurrealDB adapter implements ports — it does not rewrite the domain.

The port traits, by family:

| Family | Port trait(s) | Lives in |
| --- | --- | --- |
| Memory | `MemoryService` (and the lower-level `MemoryRepository` + `MemoryEventRepository`) | `engram-memory` |
| Knowledge | `KnowledgeRepository`, `KnowledgeGraphRepository`, `OntologyRepository`, `TaxonomyRepository` | `engram-knowledge` |
| Belief | `BeliefRepository` | `engram-belief` |
| Hierarchy | `HierarchyRepository` | `engram-hierarchy` |
| Vectors | `VectorIndex` | `engram-retrieval` |

The provider holds each family as `Arc<dyn Trait>` and reports it `Supported` only when its conformance check passes. See the [provider handles reference](../tutorials/use-engram-as-memory-layer.md#reference-provider-handles) for the operations on each.

You do not have to implement every family. Implement the ones your host needs; the rest stay `Unsupported` and their handles stay `None`.

## Step 1 — Create the adapter crate

Mirror the SQLite adapter layout. For a SurrealDB memory store:

```
adapters/surreal/
  memory/
    Cargo.toml
    src/
      lib.rs
      store.rs       # raw repository: MemoryRepository + MemoryEventRepository
      service.rs     # orchestration: MemoryService
```

`adapters/surreal/memory/Cargo.toml`:

```toml
[package]
name = "engram-store-surreal-memory"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
async-trait.workspace = true
engram-domain = { path = "../../../core/domain" }
engram-memory = { path = "../../../core/memory" }
engram-runtime = { path = "../../../core/runtime" }
surrealdb = "1"        # pin the version you target
serde_json.workspace = true
```

Register the crate in the root `Cargo.toml` workspace:

```toml
[workspace]
members = [
    # …existing members…
    "adapters/surreal/memory",
]
```

Confirm it resolves:

```bash
cargo check -p engram-store-surreal-memory
```

## Step 2 — Hold a SurrealDB connection behind the port

Keep one `surrealdb::Surreal` client behind your store struct, the way `SqlMemoryStore` holds one SQLite connection. The store is the raw repository; the service composes it.

```rust
// src/store.rs
use engram_domain::*;
use engram_runtime::CoreError;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;

/// Raw SurrealDB-backed memory repository.
pub struct SurrealMemoryStore {
    db: Surreal<Client>,
}

impl SurrealMemoryStore {
    pub async fn connect(endpoint: &str, namespace: &str, database: &str) -> CoreResult<Self> {
        let db = Surreal::new::<Client>(endpoint)
            .await
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-surreal-memory".to_string(),
                message: format!("connect: {e}"),
            })?;
        db.use_ns(namespace).use_db(database).await.map_err(|e| CoreError::Adapter {
            adapter: "engram-store-surreal-memory".to_string(),
            message: format!("use_ns/db: {e}"),
        })?;
        Ok(Self { db })
    }
}
```

## Step 3 — Define the schema and implement `MemoryRepository`

The SQLite adapter stores each record as contract JSON plus scope columns for filtering. Do the same in SurrealDB: one table per record type, scope fields indexed for the visibility check.

```sql
-- illustrative SURQL; run once at connection time
DEFINE TABLE memories SCHEMALESS;
DEFINE INDEX memories_tenant ON memories COLUMNS tenant;
DEFINE INDEX memories_subject ON memories COLUMNS subject;
DEFINE INDEX memories_workspace ON memories COLUMNS workspace;
DEFINE TABLE memory_events SCHEMALESS;
DEFINE INDEX memory_events_memory_id ON memory_events COLUMNS memory_id;
```

Then implement the repository trait. The contract is: persist the full `MemoryRecord` as JSON, keep scope columns for the visibility filter, and never let one tenant's query touch another tenant's rows.

```rust
#[async_trait::async_trait]
impl engram_memory::MemoryRepository for SurrealMemoryStore {
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord> {
        let mut doc = serde_json::to_value(&record).map_err(|e| CoreError::Adapter {
            adapter: "engram-store-surreal-memory".to_string(),
            message: format!("serialize: {e}"),
        })?;
        // Bind scope columns alongside the JSON so retrieval can filter on them.
        doc["tenant"] = serde_json::json!(record.scope.tenant);
        doc["subject"] = serde_json::json!(record.scope.subject);
        doc["workspace"] = serde_json::json!(record.scope.workspace);

        let _: Option<serde_json::Value> = self
            .db
            .create(("memories", record.id.to_string()))
            .content(doc)
            .await
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-surreal-memory".to_string(),
                message: format!("create: {e}"),
            })?;
        Ok(record)
    }

    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>> {
        let mut result = self
            .db
            .query("SELECT * FROM type::thing('memories', $id)")
            .bind(("id", id.to_string()))
            .await
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-surreal-memory".to_string(),
                message: format!("get: {e}"),
            })?;
        let row: Option<serde_json::Value> = result.take(0).map_err(|e| CoreError::Adapter {
            adapter: "engram-store-surreal-memory".to_string(),
            message: format!("take: {e}"),
        })?;
        let Some(row) = row else { return Ok(None) };
        // Enforce scope visibility in the adapter — never return a record the
        // caller's scope cannot see, even if the query found it.
        if !scope_allows(scope, &row) {
            return Ok(None);
        }
        let record: MemoryRecord =
            serde_json::from_value(row).map_err(|e| CoreError::Adapter {
                adapter: "engram-store-surreal-memory".to_string(),
                message: format!("deserialize: {e}"),
            })?;
        Ok(Some(record))
    }
    // append_event, update_memory_status, … mirror the same shape.
}
```

The `scope_allows` check is the load-bearing rule. Every retrieve path must filter by the caller's scope; a memory written under tenant `my-agent` is invisible to tenant `other-agent`. The SQLite adapter does this in SQL `WHERE` clauses — your adapter must do the equivalent, whether in SURQL or in Rust after the read.

## Step 4 — Implement `MemoryService` (what the provider holds)

The provider holds `Arc<dyn MemoryService>`, not the raw repository. `MemoryService` is the orchestration trait: `write_memory`, `retrieve`, `forget`. The SQLite adapter splits this into a `Store` (raw repo) and a `Service` (validation, policy, id generation, then delegate to the store). You can mirror that split, or implement `MemoryService` directly.

The minimum: produce a struct that implements `MemoryService` and returns the same shapes the SQLite service does.

```rust
// src/service.rs
use engram_memory::MemoryService;

pub struct SurrealMemoryService {
    store: SurrealMemoryStore,
}

#[async_trait::async_trait]
impl MemoryService for SurrealMemoryService {
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse> {
        // 1. Generate the memory id (the SQLite adapter uses a SequentialIdGenerator).
        // 2. Build the MemoryRecord + MemoryEvent from the request.
        // 3. Persist both (put_memory + append_event).
        // 4. Return WriteMemoryResponse { record, event, deduplicated: Some(false) }.
        todo!("see SqlMemoryService::write_memory for the orchestration shape")
    }

    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload> {
        // Load candidate records visible to request.scope, rank them, return ContextPayload.
        todo!()
    }

    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult> {
        // Apply delete/tombstone/redact per request.mode within request.scope.
        todo!()
    }
}
```

The orchestration logic (id generation, record/event construction, retrieval ranking) is the same for every backend. Mirror `SqlMemoryService` in `adapters/memory/sqlite/src/` rather than reinventing it — the only thing that changes is the storage calls underneath.

## Step 5 — Implement the other families you need

Repeat the pattern per family. Each is one trait (or a small group) on one store struct:

- **Knowledge** — implement `KnowledgeRepository`, `KnowledgeGraphRepository`, `OntologyRepository`, and `TaxonomyRepository` on a single `SurrealKnowledgeStore`, exactly as `SqlKnowledgeStore` does. The four share graphs, entities, chunks, and concept schemes.
- **Belief** — `BeliefRepository` on `SurrealBeliefStore`. Preserve the valid-time vs record-time distinction: a `get_belief` query with `recorded_at` set must return `InvalidRequest`, not a historical row.
- **Hierarchy** — `HierarchyRepository` on `SurrealHierarchyStore`.
- **Vectors** — `VectorIndex` on a SurrealDB-backed index (SurrealDB has vector search; wire it as the `embedding_space` identity check plus `insert`/`search`). The embedding-space match is mandatory — reject inserts and searches whose embedding space differs from the index's.

You do not need all of them. A host that only wants memory + knowledge on SurrealDB implements those two and leaves the rest `Unsupported`.

## Step 6 — Wire it into the provider

The provider is assembled in `adapters/integration/src/wiring.rs` by `bootstrap_provider`, which constructs the SQLite adapters. You have two options.

**Option A — your own bootstrap (recommended, no fork).** Build the provider yourself with `EngramProviderBuilder`, attaching your SurrealDB handles:

```rust
use engram_integration::{CapabilityReport, EngramProviderBuilder};
use engram_domain::{CapabilityReason, CapabilityState};

pub async fn bootstrap_surreal_provider(
    endpoint: &str,
    ns: &str,
    db: &str,
) -> engram_runtime::CoreResult<engram_integration::EngramProvider> {
    let memory = SurrealMemoryService::new(connect(endpoint, ns, db).await?).await?;

    // Build a capability report that reflects what you actually implemented.
    let unsupported = CapabilityState::Unsupported {
        reason: CapabilityReason::UnsupportedStoreFamily,
    };
    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .knowledge(unsupported.clone())
        .graph(unsupported.clone())
        .ontology(unsupported.clone())
        .taxonomy(unsupported.clone())
        .beliefs(unsupported.clone())
        .hierarchy(unsupported.clone())
        .retrieval(unsupported.clone())
        .vectors(unsupported.clone())
        .migration(unsupported)
        .build();

    Ok(EngramProviderBuilder::new(report).memory(std::sync::Arc::new(memory)).build())
}
```

A host calls your `bootstrap_surreal_provider` instead of `bootstrap_provider` and gets the same `EngramProvider` facade — every handle, capability check, and diagnostic works unchanged.

**Option B — edit `bootstrap_provider`.** Replace the `Sql*Store::open_file(...)` construction calls with your SurrealDB constructors. Only do this if you want SurrealDB to be the built-in default for every host.

## Step 7 — Verify against the conformance contract

The conformance fixtures in `adapters/integration/src/fixtures/` define what "correct" means for each family: write, retrieve, scope isolation, and (for memory) forget. They run against the SQLite adapter specifically — there is no generic "run conformance against any store" harness yet.

To prove your SurrealDB adapter is correct, mirror each fixture's operations as a test against your store. For memory, that is: write a memory under one scope, retrieve it back, confirm a different scope sees nothing, and forget it.

```rust
#[tokio::test]
async fn surreal_memory_round_trips_and_isolates_scope() {
    let svc = SurrealMemoryService::new(connect("localhost:8000", "test", "test").await.unwrap()).await.unwrap();
    // write under tenant-a, retrieve under tenant-a (hit), tenant-b (miss), forget.
    // This is the same sequence adapters/integration/src/fixtures/memory.rs runs
    // against SqlMemoryStore — your adapter must pass the equivalent.
}
```

If your adapter passes the same sequence the SQLite fixture does, it satisfies the memory contract. Do the same for each family you implemented.

## Common pitfalls

- **Scope leakage.** The single most common adapter bug is a retrieve path that returns a record the caller's scope should not see. Filter on every retrieve, in SURQL or in Rust. The SQLite adapter's `WHERE tenant = ? AND (subject IS NULL OR subject = ?)` clauses exist for this reason.
- **Forgetting the service-vs-repository split.** The provider wants `MemoryService`, not `MemoryRepository`. If you only implement the repository, the provider has nothing to hold. Implement the service trait (or compose a service over your repository, as the SQLite adapter does).
- **Mixing embedding spaces.** A vector adapter that accepts any 384-dim vector regardless of model will silently return wrong results. Validate the full `EmbeddingSpace` identity (provider + model + dimensions + prompt profile) on insert and search.
- **Putting domain logic in the adapter.** Ranking, consolidation, policy decisions, and belief reconciliation belong in the core, not your store. If you find yourself reimplementing retrieval fusion or contradiction detection in SURQL, stop — that logic already lives in `engram-*` core crates. Your adapter persists and fetches; it does not decide.
- **One crate per family, or one shared crate?** The SQLite layout uses one crate per family (`engram-store-sql`, `engram-store-knowledge-sqlite`, …) because each can evolve independently. You can do the same, or collect your SurrealDB stores into fewer crates — but keep the family boundaries inside, so a host can depend on only the families it uses.

## See also

- [Add memory to a Rust agent with Engram](../tutorials/use-engram-as-memory-layer.md) — the on-ramp this guide extends, including the provider-handles reference
- [How repos get indexed](../explanation/how-repos-get-indexed.md) — how Engram turns a repository into the knowledge graph your adapter would store
- The SQLite adapters under `adapters/memory/sqlite/`, `adapters/knowledge/sqlite/`, `adapters/orchestration/belief-sqlite/`, `adapters/hierarchy/sqlite/` — the reference implementations to mirror
- `AGENTS.md` boundary rules — what an adapter may and may not depend on
