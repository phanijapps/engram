#!/usr/bin/env python3
"""Guard TypeScript packages against reimplementing Rust-owned behavior."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TRANSPORT = ROOT / "packages/node/src/transport.ts"
SCAN_ROOTS = [ROOT / "packages", ROOT / "demo/backend/src"]

REQUIRED_NATIVE_CALLS = [
    "writeMemoryJson",
    "retrieveJson",
    "forgetJson",
    "validateTaxonomyProposalJson",
    "validateParentageJson",
    "planJson",
    "architectureCoverageJson",
    "detectContradictionsJson",
]

FORBIDDEN_IMPLEMENTATIONS = [
    "cosineSimilarity",
    "rankEmbeddingCandidates",
    "intervalContains",
    "validateHierarchyParentage",
    "planConsolidationOperations",
    "summarizeArchitectureCoverage",
]


def main() -> int:
    errors: list[str] = []
    transport = TRANSPORT.read_text(encoding="utf-8")
    for native_call in REQUIRED_NATIVE_CALLS:
        if f"this.engine.{native_call}" not in transport:
            errors.append(f"{TRANSPORT}: missing native delegation to {native_call}")

    forbidden = re.compile(
        r"\b(?:function|const|let|var)\s+("
        + "|".join(re.escape(name) for name in FORBIDDEN_IMPLEMENTATIONS)
        + r")\b"
    )
    for root in SCAN_ROOTS:
        for path in root.rglob("*.ts"):
            if should_skip(path):
                continue
            text = path.read_text(encoding="utf-8")
            for match in forbidden.finditer(text):
                errors.append(
                    f"{path}: TypeScript appears to implement Rust-owned behavior "
                    f"{match.group(1)}"
                )

    if errors:
        print("TypeScript native delegation check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("TypeScript native delegation check passed")
    return 0


def should_skip(path: Path) -> bool:
    parts = set(path.parts)
    return (
        "dist" in parts
        or "node_modules" in parts
        or path.name.endswith(".d.ts")
        or "/test/" in path.as_posix()
    )


if __name__ == "__main__":
    raise SystemExit(main())
