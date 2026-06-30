#!/usr/bin/env bash
# Forbidden-import gate (RFC 0003 D3): the knowledge SQLite adapter must NOT
# depend on the memory SQL adapter or the vector adapter. Each storage concern
# stays behind its own crate boundary so a durable knowledge backend can move to
# Postgres / a graph store without coupling. A failure here means cross-adapter
# SQL coupling crept in — fix it by removing the dependency, not by relaxing this
# gate.
set -euo pipefail

out="$(cargo tree -p engram-store-knowledge-sqlite 2>/dev/null || true)"
if echo "$out" | grep -qE '(^|[^-])engram-store-(sql|vector|memory)'; then
  echo "check-knowledge-sqlite-isolation: FAILED — engram-store-knowledge-sqlite" >&2
  echo "depends on a sibling store adapter:" >&2
  echo "$out" | grep -E 'engram-store-(sql|vector|memory)' >&2
  exit 1
fi
echo "check-knowledge-sqlite-isolation: ok"
