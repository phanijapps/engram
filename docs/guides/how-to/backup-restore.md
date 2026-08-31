# Backup and restore

## Postgres (pgvector backend)

The store is one database; standard Postgres tooling applies.

```bash
# Backup (custom format, compressed)
docker exec engram-pgvector pg_dump -U engram -Fc -f /tmp/engram.dump engram
docker cp engram-pgvector:/tmp/engram.dump ./engram-$(date +%F).dump

# Restore into a fresh database
docker exec -i engram-pgvector psql -U engram -c "CREATE DATABASE engram_restore;"
cat engram-<date>.dump | docker exec -i engram-pgvector pg_restore -U engram -d engram_restore
```

Restore-verify: open `engram-mcp --backend pgvector --pg-connection-string
.../engram_restore`, check `capability_report` is clean, run a `recall`
spot-check, and confirm `SELECT * FROM schema_meta` reports the expected
schema version. Vectors are part of the dump — no reindex needed after a
full restore.

## SQLite (default backend)

The store is a single WAL-mode database file. Use the online backup API (not
a raw file copy while the server runs):

```bash
# Consistent online backup
sqlite3 ~/.engram/<store>/engram_data.db "VACUUM INTO '/path/to/backup-$(date +%F).db'"

# Restore: stop the server, replace the file, restart
cp /path/to/backup-<date>.db ~/.engram/<store>/engram_data.db
```

Also preserve `<storage>/lexical/` (the Tantivy lexical index) and
`<storage>/scan-manifests/` — losing them costs a full re-scan and a
`reindex` drain, not data.

## Schedule

Pre-1.0 guidance: nightly backup + a restore-verify in a scratch directory
weekly; the runbooks above are small enough to script as-is.
