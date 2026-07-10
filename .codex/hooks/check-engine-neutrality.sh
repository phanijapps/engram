#!/usr/bin/env bash
# check-engine-neutrality.sh — ADR-0022 rule-1 gate (engine neutrality).
#
# Fails if an engine symbol, engine-crate import, or raw-SQL literal appears in
# the engine-neutral layers: the 8 clean port-trait crates (domain, memory,
# knowledge, retrieval, belief, hierarchy, consolidation, orchestration) plus
# exactly core/integration/src/provider.rs and core/integration/src/capability.rs.
#
# Enforcing ADR-0022 here is what makes "swap the storage backend by config,
# not by rewrite" literally true: no neutral layer may name an engine type.
#
# Deferred-debt layers are intentionally NOT gated (see docs/backlog.md,
# "provider-sdk-capability-report"): engram-runtime (home-grown SqliteOpenOptions
# and the SQL-redaction regex), core/integration/src/config.rs (SqliteStorageLayout),
# core/eval, and bindings/node.
#
# Usage:
#   check-engine-neutrality.sh            # scan the default gated surface
#   check-engine-neutrality.sh <path>...  # scan the given paths (for self-test)
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

if [ "$#" -gt 0 ]; then
  GATED_PATHS=("$@")
else
  GATED_PATHS=(
    "$ROOT/core/domain/src"
    "$ROOT/core/memory/src"
    "$ROOT/core/knowledge/src"
    "$ROOT/core/retrieval/src"
    "$ROOT/core/belief/src"
    "$ROOT/core/hierarchy/src"
    "$ROOT/core/consolidation/src"
    "$ROOT/core/orchestration/src"
    "$ROOT/core/integration/src/provider.rs"
    "$ROOT/core/integration/src/capability.rs"
    "$ROOT/core/integration/src/provenance.rs"
    "$ROOT/core/integration/src/batch.rs"
    "$ROOT/core/integration/src/recall.rs"
    "$ROOT/core/integration/src/export_import.rs"
    "$ROOT/core/integration/src/observability.rs"
  )
fi

# Forbidden pattern classes (ADR-0022 rule 1):
#  (a) engine type names: Sql*, Pg*, Surreal*, Lance*, Tantivy* …
#  (b) engine-crate imports/refs: `use`/`extern crate` of rusqlite, sqlx,
#      sqlite-vec/sqlite_vec, pgvector, tantivy, engram-store-* (anchored at the
#      `use`/`extern` token, not column 0, so indented uses fire too), plus bare
#      engine path references like `rusqlite::Connection::open()`.
#  (c) raw-SQL string literals: a quoted string containing a SQL DML/DDL/PRAGMA
#      keyword (SELECT/INSERT/UPDATE/DELETE/CREATE/DROP/ALTER/PRAGMA/VACUUM/
#      REINDEX/ANALYZE/ATTACH/DETACH/BEGIN/COMMIT/ROLLBACK).
PATTERN='\bSql[A-Z][A-Za-z0-9_]*\b|\bPg[A-Z][A-Za-z0-9_]*\b|\bSurreal[A-Z][A-Za-z0-9_]*\b|\bLance[A-Z][A-Za-z0-9_]*\b|\bTantivy[A-Z][A-Za-z0-9_]*\b'
PATTERN="$PATTERN|(use |extern crate )[^;]*\b(rusqlite|sqlx|sqlite[-_]vec|pgvector|tantivy|engram[-_]store[-_][a-z0-9_-]+)\b"
PATTERN="$PATTERN|\b(rusqlite|sqlx|tantivy|sqlite_vec|pgvector|engram_store_[a-z_]+)::[A-Za-z0-9_:]+"
PATTERN="$PATTERN|\"[^\"]*\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|VACUUM|REINDEX|ANALYZE|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK)\b"

status=0
for p in "${GATED_PATHS[@]}"; do
  [ -e "$p" ] || continue
  if hits="$(grep -rnE "$PATTERN" "$p" 2>/dev/null)"; then
    echo "ADR-0022 rule-1 violation (engine neutrality) in $p:" >&2
    echo "$hits" >&2
    status=1
  fi
done

exit $status
