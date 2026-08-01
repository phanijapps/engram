# engram-backend-pgvector

The **pgvector (Postgres) backend recipe** — ADR-0022's "backend = recipe crate."
This is the only place a "pgvector" backend identity exists: it owns Postgres
connection lifecycle, schema application, adapter-cell composition, and
per-engine tests. Hosts open a Postgres-backed provider via [`open`].

## Why a recipe (not a module in the SDK facade)

`engram-integration` (the SDK facade) cannot depend on this crate without a
dependency cycle (the recipe depends on integration for `EngramProvider` + the
port traits). So the recipe — not `EngramProvider::open` — is the pgvector host
entry. `EngramProvider::open` stays engine-neutral (sqlite default) and **rejects**
a config carrying `pgvector_connection_string`, pointing here.

## Open a provider

```rust
use engram_backend_pgvector::open;
use engram_integration::EngramConfig;

let config = EngramConfig::new(/* … */)
    .with_pgvector("postgres://engram:engram@localhost:5432/engram");
let provider = open(&config)?;
```

The N-API binding routes a pgvector config here automatically when built with the
`pgvector` feature: `cargo build -p engram-node --features fastembed,pgvector`.

## Run it (Docker)

```bash
docker compose -f docs/how-to-pg/docker-compose.yaml up -d   # pgvector/pgvector:pg17
cargo test -p engram-backend-pgvector -- --ignored            # capabilities + round-trip
```

Connection string: `postgres://engram:engram@localhost:5432/engram`.

## Scope

One Postgres holds the graph (entities/relationships/graphs), chunks/documents/
sources, memory records, and embeddings (pgvector type). SQLite stays the
local/single-user default; backend is chosen by config. Extracting
`backends/sqlite` (full neutrality) is deferred — see
`docs/backlog.md#backends-sqlite-extraction`. Reusing the conformance suite
against pgvector is also deferred (`ConformanceHarness::new()` is sqlite-coupled;
it needs a provider-injection refactor — see
`docs/backlog.md#pgvector-conformance-suite`).
