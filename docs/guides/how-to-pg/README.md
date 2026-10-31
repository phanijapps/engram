# pgvector development environment

A Docker Compose file that runs Postgres 17 + pgvector for engram's second
storage backend (RFC-0017 Phase A).

## Start

```bash
docker compose -f docs/how-to-pg/docker-compose.yaml up -d
```

## Connection string

```
postgres://engram:engram@localhost:5432/engram
```

## Verify

```bash
docker exec engram-pgvector psql -U engram -d engram -c "SELECT '[1,2,3]'::vector(3);"
```

## Stop

```bash
docker compose -f docs/how-to-pg/docker-compose.yaml down
```

Data persists in the `engram-pgvector-data` Docker volume across restarts.

## Schema

The schema DDL lives in `adapters/pgvector/src/schema.rs` (`schema_sql(dimensions)`).
Run it on first connect:

```bash
# Apply the schema (substitute dimensions, e.g. 384 for BGE-small):
docker exec -i engram-pgvector psql -U engram -d engram < <(cargo run -p engram-store-pgvector --bin schema_init 2>/dev/null || echo "-- run schema_sql(384) output")
```

The DDL creates tables for memories, knowledge (sources/documents/chunks/
entities/relationships/graphs), vectors (pgvector type + HNSW index), and
beliefs/contradictions — mirroring the SQLite schema with Postgres-native
types (JSONB, TIMESTAMPTZ).

## Driver note

The Postgres driver (tokio-postgres or sqlx) lands in T2. The `postgres 0.12`
sync crate does not compile on Rust 1.85 (edition 2024 type-inference change).
T2 resolves this via tokio-postgres + a shared tokio runtime in the adapter.
