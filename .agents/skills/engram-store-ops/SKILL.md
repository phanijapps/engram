---
name: engram-store-ops
description: Use when operating engram stores on this machine — inventory, wipes, backups, scope wiring, the MCP/BFF two-database trap, or deciding which store a query hits. Triggers on "which store", "clean the store", "start the server", "point the UI at", "backup", "where does the data live". Encodes the operational map established 2026-08-28: four SQLite stores, one Postgres, their scopes, and the traps that bit.
---

# Skill: engram-store-ops

## Inventory (this machine)

| Store | Scope | Contents | Wipe policy |
|---|---|---|---|
| `~/.engram/agentzero` | `default/agentzero` | THE shared agent workspace: session memories + scans of this repo + legacy zbot ×2 + NULL-scope rows | **Never casually** — session memory lives here |
| `~/.engram/mem-alpha-self` | `default/mem-alpha-self` | this repo only; the self-index + temporal baseline | No — wipes reset `whats_changed` |
| `~/.engram/springboot-demo` | `default/spring-boot-demo` | single repo, clean | Fine (kill BFF first) |
| `~/.engram/dreamhouse-lwc` | `default/dreamhouse-lwc` | single repo, clean | Fine |
| Docker `engram-pgvector` | — | Postgres test/recipe DB (`docs/how-to-pg/`) | TRUNCATE between tests |

Stores do not cross-talk: each MCP launch fuses per project.

## Servers

- **Harness MCP** (memory layer, workspace `agentzero`): configured by
  `.mcp.json`, auto-restarts from `target/release/engram-mcp`. Rebuild the
  binary and it picks up on next spawn.
- **Ad-hoc stdio**: `scripts/dev/mcp_driver.py` (see engram-live-verify).
- **UI stack** (engram-cc): BFF `:3001` + vite `:5173`, pointed at a store
  via env — see below.

## The traps

1. **Two DB files, one storage dir**: the MCP opens `engram_data.db`
   (default `--db-file`); the BFF reads whatever `ENGRAM_DB_FILE` says. They
   are separate files — a mismatch is a silent empty UI, not an error.
2. **BFF workspace must match the scanned workspace EXACTLY** — the
   node:sqlite path is strict (`spring-boot-demo` ≠ `springboot-demo`), while
   the Rust facade is lenient (NULL-OR). Stats can look fine while strict
   queries return nothing. When pointing the UI at a store:
   ```bash
   ENGRAM_STORAGE=<store> ENGRAM_DB_FILE=engram_data.db \
   ENGRAM_TENANT=default ENGRAM_WORKSPACE=<exact-project> PORT=3001 \
   npx tsx engram-cc/backend/src/index.ts
   ```
3. **Wiping**: kill the BFF first (`lsof -ti :3001 | xargs -r kill`) — an
   open handle silently blocks `rm -rf`. Verify with `[ -d path ]` after.
4. **Vector accounting**: the live vector table is a vec0 virtual table;
   shadow-table reads mislead. Trust `reindex`'s `embedded N (D new + R
   reused)` output. Draining a backlog: repeat `reindex` until "backlog
   drained" (durable dedup makes repeats cheap).
5. **Fresh boot of a missing `--storage` path** creates the tree (fixed
   2026-08-27) — but check STDERR for `knowledge_store=false`-style
   degradation before trusting capabilities.

## Backups

`docs/guides/how-to/backup-restore.md` — sqlite `VACUUM INTO` (online,
consistent; never raw-copy a live WAL), pg_dump for Postgres; preserve
`lexical/` + `scan-manifests/` sidecars (losing them costs a re-scan + a
reindex drain, not data).
