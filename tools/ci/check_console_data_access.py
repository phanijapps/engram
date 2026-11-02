#!/usr/bin/env python3
"""Console data-access gate (job 012-admin-console-v1; PRD-107 / B-4 / NFR-007).

The console must reach data ONLY through the engine surface (`@engram/node`).
This gate fails when a console production source file holds an engine driver
(store client libraries, direct store file access) or a SQL literal — either
one means the console became a second data-access layer (RISK-001).

Scanned surface: apps/console/src/** (the shipped UI) and
apps/console/server/*.mjs (the server core + entry point). Tests are
deliberately NOT scanned: the count-truth and readonly-digest tests query
stores directly BY DESIGN to prove engine results against ground truth; that
license lives in tests, never in shipped source.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT, fail, ok  # noqa: E402

NAME = "console-data-access"

SCAN_ROOTS = [
    "apps/console/src",
    "apps/console/server",
]

# (a) engine drivers and direct store access — by IMPORT or module-path
# reference. The bare words sqlite/pgvector as backend flag values are
# legitimate launch vocabulary (plan DEC-007: backend selection is
# flag-passed config, never console data access), so this matches
# imports/require/module paths — not mentions.
DRIVER_PATTERN = re.compile(
    r"\b(?:import|from|require|import\()\s*\(?['\"]"
    r"(node:sqlite|sqlite3|better-sqlite3|pg|pg-pool|pgvector|rusqlite|sqlx|"
    r"libpq|psql|mongodb|surrealdb|sqlite-vec)"
    r"['\"]"
    r"|\b(engram_store_[a-z_]+|engram-store-[a-z-]+)::?"
    r"|\bDatabaseSync\b|\bClient\.connect\("
)
# (b) connection strings / store file paths.
CONNECTION_PATTERN = re.compile(
    r"\bpostgres(ql)?://|\bmysql://|\.db\b.*open|DatabaseSync\("
)
# (c) SQL literals in strings — faithful to the accepted engine-neutrality set.
SQL_PATTERN = re.compile(
    r"\"[^\"]*\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|VACUUM"
    r"|REINDEX|ANALYZE|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK)\b"
    r"|'[^']*\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|PRAGMA|VACUUM"
    r"|REINDEX|ANALYZE|ATTACH|DETACH|BEGIN|COMMIT|ROLLBACK)\b"
)


def scan_paths(paths: list[Path]) -> list[str]:
    violations: list[str] = []
    for p in paths:
        if not p.exists():
            violations.append(f"declared console source path missing on disk: {p}")
            continue
        files = [p] if p.is_file() else [f for f in p.rglob("*") if f.is_file()]
        for f in files:
            try:
                text = f.read_text(errors="replace")
            except OSError as error:
                violations.append(f"{f}: unreadable: {error}")
                continue
            for i, line in enumerate(text.splitlines(), 1):
                for label, pattern in (
                    ("engine driver", DRIVER_PATTERN),
                    ("store connection", CONNECTION_PATTERN),
                    ("SQL literal", SQL_PATTERN),
                ):
                    if pattern.search(line):
                        try:
                            location = f.relative_to(ROOT)
                        except ValueError:
                            location = f
                        violations.append(
                            f"{location}:{i}: {label}: {line.strip()[:120]}"
                        )
    return violations


def scan(root: Path = ROOT) -> list[str]:
    return scan_paths([root / p for p in SCAN_ROOTS])


def main() -> int:
    violations = scan()
    if violations:
        print(f"{NAME}: console source holds direct data access:", file=sys.stderr)
        return fail(NAME, violations)
    return ok(NAME, f"{len(SCAN_ROOTS)} roots clean (engine surface only)")


if __name__ == "__main__":
    sys.exit(main())
