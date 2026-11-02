#!/usr/bin/env python3
"""Engine-neutrality gate (ADR-0022 rule-1 / constitution rail 01 / EN-001).

Ported 1:1 from check-engine-neutrality.sh (2026-10-31 python consolidation;
patterns are byte-faithful to the accepted shell gate).

Fails when an engine symbol, engine-crate import, or raw-SQL literal appears
in the gated neutral layers. Deferred-debt layers are intentionally NOT gated
(see docs/history/backlog.md "provider-sdk-capability-report"): engram-runtime,
core/eval, core/integration/src/config.rs, and bindings/node.

Usage:
  check_engine_neutrality.py            # scan the default gated surface
  check_engine_neutrality.py <path>...  # scan given paths (self-test)
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT, fail, ok  # noqa: E402

NAME = "engine-neutrality"

GATED_PATHS = [
    "core/domain/src",
    "core/memory/src",
    "core/knowledge/src",
    "core/retrieval/src",
    "core/belief/src",
    "core/hierarchy/src",
    "core/consolidation/src",
    "core/procedures/src",
    "core/proposals/src",
    "core/integration/src/provider.rs",
    "core/integration/src/capability.rs",
    "core/integration/src/provenance.rs",
    "core/integration/src/batch.rs",
    "core/integration/src/recall.rs",
    "core/integration/src/export_import.rs",
    "core/integration/src/observability.rs",
    "core/integration/src/knowledge_query.rs",
    "core/integration/src/lexical_feed.rs",
    "core/integration/src/scope_inventory.rs",
    "core/packs/src",
    "mcp/engram-mcp/src",
]

# (a) engine type names  (b) engine-crate imports  (c) engine path refs
# (d) raw-SQL string literals — faithful to the accepted shell gate.
PATTERN = (
    r"\bSql[A-Z][A-Za-z0-9_]*\b|\bPg[A-Z][A-Za-z0-9_]*\b|\bSurreal[A-Z][A-Za-z0-9_]*\b"
    r"|\bLance[A-Z][A-Za-z0-9_]*\b|\bTantivy[A-Z][A-Za-z0-9_]*\b"
    r"|(use |extern crate )[^;]*\b(rusqlite|sqlx|sqlite[-_]vec|pgvector|tantivy"
    r"|engram[-_]store[-_][a-z0-9_-]+|surreal|surrealdb)\b"
    # bare surreal:: path-refs intentionally NOT denied (facade dispatch)
    r"|\b(rusqlite|sqlx|tantivy|sqlite_vec|pgvector|engram_store_[a-z_]+)::[A-Za-z0-9_:]+"
    r"|\"[^\"]*\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|VACUUM"
    r"|REINDEX|ANALYZE|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK)\b"
)
RE = re.compile(PATTERN)


def scan(paths: list[Path]) -> list[str]:
    violations: list[str] = []
    for p in paths:
        if not p.exists():
            # EN-004 fail-closed (DW-7 closed 2026-10-31): a declared gated path
            # that is missing narrows enforcement silently — that is a violation.
            violations.append(f"declared gated path missing on disk: {p}")
            continue
        files = [p] if p.is_file() else [f for f in p.rglob("*") if f.is_file()]
        for f in files:
            try:
                for i, line in enumerate(
                    f.read_text(errors="replace").splitlines(), 1
                ):
                    if RE.search(line):
                        violations.append(f"{f.relative_to(ROOT)}:{i}: {line.strip()[:120]}")
            except OSError:
                continue
    return violations


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    paths = [Path(a) for a in args] if args else [ROOT / p for p in GATED_PATHS]
    violations = scan(paths)
    if violations:
        print(f"{NAME}: ADR-0022 rule-1 violation(s):", file=sys.stderr)
        return fail(NAME, violations)
    return ok(NAME, "gated surface clean")


if __name__ == "__main__":
    sys.exit(main())
