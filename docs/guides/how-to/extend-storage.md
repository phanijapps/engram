# Extend the storage layer — add a new backend

> How to add a new storage engine to engram (e.g. `engram-store-postgres`,
> `engram-store-lancedb`, `engram-store-mixed`). For the worked SURQL example,
> see [Use SurrealDB as the Engram store](./build-a-surrealdb-store.md); for the
> architecture this fits into, see the
> [architecture overview](../../architecture/overview.md).

This guide is for a Rust developer who has engram integrated and wants a
different storage engine. If you are new to engram, start with
[Add memory to a Rust agent with Engram](../tutorials/use-engram-as-memory-layer.md).

## The contract: a backend is one crate + one recipe

Engram separates **engine** (a per-capability adapter cell behind a core port)
from **backend** (the composition that wires cells into an `EngramProvider`).
Per [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md) (amended):

- **One crate per backend.** ALL database operations for an engine live in a
  single crate `engram-store-<engine>` at `adapters/<engine>/` — every capability
  cell (memory, knowledge, belief, hierarchy, vectors) sharing one connection.
  Existing examples: `engram-store-sqlite`, `engram-store-surreal`.
- **The recipe is a feature-gated submodule of `engram-integration`**, not a
  separate crate. The `bootstrap_<engine>` function returns an `EngramProvider`
  — a type owned by `core/integration` — so a separate backend *crate* would form
  a Cargo cycle. The recipe lives at `core/integration/src/<engine>/bootstrap.rs`.
- **No engine type may leak into a neutral layer.** `engram-domain`, the other
  `core/*` port crates, `engram-integration`'s neutral facade files, and the
  N-API binding must never name `Sql*`, `Surreal*`, `pgvector`, … The only place
  an engine name may appear in those layers is as a config string. A neutrality
  lint (`.codex/hooks/check-engine-neutrality.sh`) enforces this — and the
  engine crate + its bootstrap submodule are exempt zones.

A consumer then changes **one config line** (a `BackendProfile` + Cargo feature)
to swap engines. No migration ships — switching backends starts a fresh store.

## What you implement: the five capability cells

A cell implements one core port (or a small group). The provider holds each as
`Arc<dyn Trait>` and reports it `Supported` only when wired.

| Family | Port trait(s) | Lives in |
| --- | --- | --- |
| Memory | `MemoryService` (over `MemoryRepository` + `MemoryEventRepository`) | `engram-memory` |
| Knowledge | `KnowledgeRepository`, `KnowledgeGraphRepository`, `OntologyRepository`, `TaxonomyRepository` | `engram-knowledge` |
| Belief | `BeliefRepository` | `engram-belief` |
| Hierarchy | `HierarchyRepository` | `engram-hierarchy` |
| Vectors | `VectorIndex` (async) | `engram-retrieval` |

You do **not** have to implement all five. Implement the ones your host needs;
the rest stay `Unsupported` and their handles stay `None`. Engine-agnostic
adapters (lexical BM25, associative-graph PPR, community-summary, cross-encoder
rerank, decay, ingest) are shared across every backend — do **not** re-implement
those in your engine crate.

## Step-by-step

The two shipped crates are your templates: `adapters/sqlite/` (relational) and
`adapters/surreal/` (graph-native). Mirror whichever is closer to your engine.

### 1. Scaffold the crate

```
adapters/<engine>/
  Cargo.toml
  src/
    lib.rs          # mod + pub use per cell (facade)
    connection.rs   # shared connection handle (lazy-open)
    util.rs         # shared helpers (scope check, error mapping)
    memory.rs       # MemoryService impl
    knowledge.rs    # the 4 knowledge ports on one store
    belief.rs       # BeliefRepository
    hierarchy.rs    # HierarchyRepository
    vector.rs       # VectorIndex
```

`Cargo.toml` depends on the core port crates only — never on another engine
crate or on `engram-integration`:

```toml
[package]
name = "engram-store-<engine>"
version.workspace = true
edition.workspace = true

[dependencies]
engram-domain = { path = "../../core/domain" }
engram-runtime = { path = "../../core/runtime" }
engram-memory = { path = "../../core/memory" }
engram-knowledge = { path = "../../core/knowledge" }
engram-belief = { path = "../../core/belief" }
engram-hierarchy = { path = "../../core/hierarchy" }
engram-retrieval = { path = "../../core/retrieval" }
async-trait.workspace = true
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# your engine client here
```

Register it in the root `Cargo.toml` `[workspace] members`, then:

```bash
cargo check -p engram-store-<engine>
```

`lib.rs` is a facade — `mod` + `pub use` per cell (see `adapters/surreal/src/lib.rs`):

```rust
pub mod memory;      pub use memory::YourMemoryService;
pub mod knowledge;   pub use knowledge::YourKnowledgeStore;
pub mod belief;      pub use belief::YourBeliefStore;
pub mod hierarchy;   pub use hierarchy::YourHierarchyStore;
pub mod vector;      pub use vector::YourVectorIndex;
pub mod connection;  pub use connection::YourConnection;
```

### 2. Hold one shared connection

One connection object is cloned (`Arc`) into every cell, exactly as
`SurrealConnection` is. If your client needs an async reactor, open it **lazily
on first use under the consumer's runtime** (e.g. `tokio::sync::OnceCell`) — the
facade's `EngramProvider::open` is sync, so a blocking open there will panic.

### 3. Implement the cells

For each family, persist the full domain record (as JSON or native rows) and
**enforce scope visibility on every retrieve path**. The load-bearing rule: a
record written under tenant `a` is invisible to tenant `b`. The SQLite adapter
does this in `WHERE` clauses; the Surreal adapter does it in SURQL + a Rust
`scope_allows` check after the read. Pick one and apply it everywhere.

- **Memory** — `MemoryService` (`write_memory`, `retrieve`, `forget`). The
  orchestration (id generation, record/event construction, ranking) is identical
  for every backend; mirror `engram-store-sqlite::memory` rather than
  reinventing it. Only the storage calls change.
- **Knowledge** — implement all four ports on one store (they share graphs,
  entities, chunks, concept schemes). Chunk visibility **inherits** from its
  owning source (chunk → document → source scope check).
- **Belief** — preserve valid-time vs record-time: a `get_belief` query with
  `recorded_at` set must return `InvalidRequest`, not a historical row. Delegate
  lifecycle/temporal logic to the shared `engram_belief` helpers.
- **Hierarchy** — delegate path navigation to the shared
  `engram_hierarchy::navigation::navigate`.
- **Vectors** — `VectorIndex` is `#[async_trait]`. Validate the full
  `EmbeddingSpace` identity (provider + model + dimensions + prompt profile) on
  insert and search; reject mismatches. The embedding *provider* (query →
  vector) is wired at the retrieval-lane level, not in the cell.

### 4. Write the bootstrap recipe

Add a feature-gated submodule in `engram-integration`. It constructs the cells
and wires them into an `EngramProvider`:

```
core/integration/src/<engine>/
  mod.rs          # `mod bootstrap;` only
  bootstrap.rs    # bootstrap_<engine>(&EngramConfig) -> CoreResult<EngramProvider>
```

`bootstrap.rs` clones the shared connection into each cell, coerces
`Arc<YourKnowledgeStore>` to each of the four knowledge trait handles, builds a
`CapabilityReport` reflecting what you wired, and assembles the provider with
`EngramProviderBuilder`. See `core/integration/src/surreal/bootstrap.rs` for the
exact shape (including `#[tokio::test]` round-trips per cell).

### 5. Declare the Cargo feature

In `core/integration/Cargo.toml`, add a feature that pulls in your crate (and
any runtime dep it needs), mirroring the `sqlite` / `surreal` features:

```toml
[features]
<engine> = ["dep:engram-store-<engine>", "dep:engram-store-lexical", …]
```

### 6. Add the config arm

Add a `BackendProfile::<Engine>` variant + a `from_profile_file` arm in
`core/integration/src/config.rs`, and route `EngramProvider::open` to your
`bootstrap_<engine>` when the profile selects your engine. Keep the neutral
facade files engine-name-free — only `config.rs` and the `<engine>/` submodule
name the engine.

### 7. Verify against the conformance contract

Mirror each capability's SQLite fixture as a test against your cells: write
under one scope, retrieve it back, confirm a different scope sees nothing,
forget/tombstone. The Surreal bootstrap module's six `#[tokio::test]`s are the
template. Then run the gates:

```bash
cargo test -p engram-integration --features <engine>
cargo check --workspace
.codex/hooks/check-engine-neutrality.sh
```

Build the N-API binding + run the TS suite too — surface parity
([AGENTS.md](../../../AGENTS.md)) requires every capability reachable from the
facade to also be reachable from the binding.

## Common pitfalls

- **Scope leakage.** The single most common adapter bug: a retrieve path that
  returns a record the caller's scope should not see. Filter on every retrieve.
- **Putting domain logic in the adapter.** Ranking, consolidation, policy, belief
  reconciliation live in the core. If you reimplement retrieval fusion or
  contradiction detection in your engine's query language, stop — that logic
  already exists in `engram-*`. Your cell persists and fetches; it does not decide.
- **Mixing embedding spaces.** A vector cell that accepts any N-dim vector
  regardless of model silently returns wrong results. Validate the full
  `EmbeddingSpace` identity.
- **A blocking open in the sync facade.** If your client needs a reactor, open
  lazily under the consumer's runtime — `block_on` from the sync facade panics.
- **Leaking the engine type into a neutral layer.** It breaks swap-by-config and
  fails the neutrality lint. The engine name lives only in the engine crate, the
  bootstrap submodule, `config.rs`, and as a config string.

## See also

- [Use SurrealDB as the Engram store](./build-a-surrealdb-store.md) — the SURQL
  worked example (`engram-store-surreal`).
- [Architecture overview](../../architecture/overview.md) — the pipeline + layer
  responsibilities this fits into.
- [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md) — engine vs
  backend, one-crate-per-backend, recipe-as-submodule.
- `adapters/sqlite/` and `adapters/surreal/` — the reference implementations.
- [`README`](../../../README.md) — project overview, use cases, and the doc map.
- `AGENTS.md` boundary rules — what an adapter may and may not depend on.
