# Use SurrealDB as the Engram store

> SurrealDB is engram's **reference second engine** — `engram-store-surreal`
> ships and is selectable by config alongside SQLite. This guide walks through
> the **SURQL-specific patterns** the shipped crate uses. For the general
> add-an-engine contract (crate layout, wiring, lints), see
> [Extend the storage layer](./extend-storage.md); for the architecture this fits
> into, see the [architecture overview](../../architecture/overview.md).

This guide is for a Rust developer who has engram integrated and wants to
understand or adapt the SurrealDB backend. If you are new to engram, start with
[Add memory to a Rust agent with Engram](../tutorials/use-engram-as-memory-layer.md).

## What ships

`engram-store-surreal` (`adapters/surreal/`) is one crate holding every
capability cell — memory, knowledge (taxonomy/ontology/graph), belief, hierarchy,
vectors — sharing **one embedded SurrealKV connection**. The `bootstrap_surreal`
recipe lives in `core/integration/src/surreal/bootstrap.rs` (a feature-gated
submodule, not a separate crate — see [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md)).
Select it with `--features surreal` and a `BackendProfile::Surreal` config line.

The contract steps below (which ports to implement, the crate layout, the wiring)
are the real shipped surface. The SURQL snippets document the patterns the crate
uses; read `adapters/surreal/src/` alongside this guide for the exact code.

## How engram separates storage from behavior

Engram never asks an adapter to own domain logic. Domain types and invariants
live in `engram-domain`; behavior ports live in the `engram-*` core crates;
storage adapters live behind those ports. The SurrealDB cells implement ports —
they do not rewrite the domain.

| Family | Port trait(s) | SurrealDB cell |
| --- | --- | --- |
| Memory | `MemoryService` | `SurrealMemoryService` |
| Knowledge | `KnowledgeRepository`, `KnowledgeGraphRepository`, `OntologyRepository`, `TaxonomyRepository` | `SurrealKnowledgeStore` (all four) |
| Belief | `BeliefRepository` | `SurrealBeliefStore` |
| Hierarchy | `HierarchyRepository` | `SurrealHierarchyStore` |
| Vectors | `VectorIndex` | `SurrealVectorIndex` |

## SURQL pattern 1 — the data-wrapper persistence shape

Each domain record is stored under a `data` field, keyed by a stable record id.
This keeps one deserialization path for every record type:

```rust
// UPSERT type::thing('<table>', $key) SET data = $record
// SELECT data FROM type::thing('<table>', $key)
```

A `DataWrapper<T>` deserialize shim (in `util.rs`) reads the `data` field back
into the domain type. The same shape serves memory records, beliefs, hierarchy
nodes, sources, documents, and chunks.

## SURQL pattern 2 — scope filtering is load-bearing

Every retrieve path filters by the caller's scope. A memory written under tenant
`a` is invisible to tenant `b`. The crate does this in SURQL plus a Rust
`scope_allows` check after the read (in `util.rs`):

```rust
// the shared visibility gate applied to every read
fn scope_allows(scope: &Scope, record: &serde_json::Value) -> bool { … }
```

For **chunks and documents**, visibility is **inherited** from the owning
source: a chunk's scope check resolves chunk → document → source. Concepts
inherit from their concept scheme. This mirrors the SQLite adapter exactly.

## SURQL pattern 3 — bi-temporal beliefs via explicit fields

`BeliefRepository` distinguishes **valid-time** (when a belief was true) from
**record-time** (when it was stored). The Surreal cell implements valid-time
lookups with explicit `valid_from` / `valid_until` fields plus a filter —
engine-neutral parity with SQLite. Record-time history queries are rejected
(`InvalidRequest`), not honored. Delegate lifecycle logic (mark-stale,
supersede, query matching) to the shared `engram_belief` helpers rather than
re-implementing them in SURQL.

> SurrealDB also supports native time-travel (`VERSION <datetime>` with a
> versioned connection), but v1 uses the explicit-field approach so the
> bi-temporal contract is identical across engines.

## SURQL pattern 4 — MTREE vectors with an inline query literal

`SurrealVectorIndex` stores vectors in a `vector_record` table with an MTREE
index and searches via the KNN operator `<|K|>`. The query vector must be inlined
as a literal — a bound `Vec<f32>` parameter is rejected by Surreal's vector
conversion:

```sql
DEFINE INDEX vec_idx ON vector_record FIELDS embedding MTREE DIMENSION 384;
SELECT target_id, embedding FROM vector_record WHERE embedding<|10|>[0.1,0.2,…];
```

The score is cosine similarity between the query and stored vector, computed in
Rust. The cell validates the full `EmbeddingSpace` identity on insert and search.

## Wiring + verification

The `bootstrap_surreal` recipe clones one `SurrealConnection` (`Arc`) into every
cell, coerces the single `SurrealKnowledgeStore` to each of the four knowledge
trait handles, builds a `CapabilityReport`, and assembles the provider. Because
the connection opens **lazily** (the Surreal SDK needs a Tokio reactor, opened
under the consumer's runtime, not from the sync facade), the conformance tests
are `#[tokio::test]`.

Six round-trip tests in `core/integration/src/surreal/bootstrap.rs` prove each
cell: memory write→retrieve→forget with scope isolation, hierarchy parent-chain,
belief valid-time lookup, knowledge chunk scope-inheritance, and vector
round-trip. Run them:

```bash
cargo test -p engram-integration --features surreal
```

## Common pitfalls

- **Scope leakage.** Filter on every retrieve, in SURQL or in Rust. This is the
  most common adapter bug.
- **A blocking open in the sync facade.** Open the connection lazily under the
  consumer's runtime; `block_on` from the sync `EngramProvider::open` panics.
- **Putting domain logic in the cell.** Ranking, consolidation, policy, and
  belief reconciliation belong in the core. The cell persists and fetches; it
  does not decide.
- **Mixing embedding spaces.** Validate the full `EmbeddingSpace` identity on
  every insert and search.

## See also

- [Extend the storage layer](./extend-storage.md) — the general add-an-engine
  contract (this guide is the SURQL instance of it).
- [Architecture overview](../../architecture/overview.md) — the pipeline + layers.
- [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md) — one crate per
  backend, recipe-as-submodule.
- `adapters/surreal/src/` — the reference implementation (read alongside this
  guide).
- [`README`](../../../README.md) — project overview, use cases, and the doc map.
- `adapters/sqlite/` — the SQLite sibling cell, for the relational variant.
