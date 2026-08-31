# Migrating a store: SQLite → Postgres (pgvector)

The executable demonstration of this runbook is
`backends/pgvector/tests/bootstrap.rs::pg_recipe_sqlite_export_import_round_trip`
(Docker-gated; see `docs/how-to-pg/` for the local Postgres). The flow:

## 1. Start the target Postgres

```bash
docker compose -f docs/how-to-pg/docker-compose.yaml up -d
# connection: postgres://engram:engram@localhost:5432/engram
```

The pgvector recipe applies its schema idempotently on first open and stamps
`schema_meta.schema_version` — check it after the cutover:

```bash
docker exec engram-pgvector psql -U engram -d engram -c "SELECT * FROM schema_meta;"
```

## 2. Export from the SQLite store

Open the SQLite store through the facade and call `ExportImport::export(scope)`
(`require_export_import()` on the provider). The result is an `ImportData`
with flattened records per family: memories, knowledge sources / documents /
chunks / entities / relationships, concept schemes.

## 3. Import into Postgres

Write the records through the recipe provider's ports — memories via
`MemoryService::write_memory` (rebuild the typed request from the record's
text + scope-JSON + policy-JSON fields), knowledge via
`KnowledgeRepository::put_source/put_document/put_chunk/put_entity/put_relationship`.

> Note: the import-record shape is intentionally flattened (text + JSON
> strings). Memory `kind` normalizes to `observation` on import; carry the
> original kind through provenance metadata if you need it verbatim.

## 4. Verify parity (the spot-check)

Run the same `recall` query against both providers and compare: the seeded
content must surface on both engines. Then verify counts:

```bash
docker exec engram-pgvector psql -U engram -d engram \
  -c "SELECT COUNT(*) FROM memories; SELECT COUNT(*) FROM knowledge_chunks;"
sqlite3 <sqlite-store>/engram_data.db \
  "SELECT COUNT(*) FROM memories; SELECT COUNT(*) FROM knowledge_chunks;"
```

## 5. Embed the migrated chunks (vectors start empty)

A backend switch starts with an empty vector index. Point `engram-mcp` at
Postgres and run `reindex` until the backlog drains (durable content-hash
dedup makes re-embedding identical texts cheap):

```
reindex: embedded 256 chunks (limit 256) (80 new + 176 reused from existing vectors)
```

## 6. Cut over

```
engram-mcp --storage <dir> --project <name> \
  --backend pgvector --pg-connection-string postgres://engram:engram@localhost:5432/engram
```

Keep the SQLite store until the parity checks satisfy you; it is the rollback
path (open it with `--backend sqlite`, the default).
