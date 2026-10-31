#!/usr/bin/env python3
"""Mechanical clean-design guardrails for Engram crate/package entry points."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CORE_LIB_LIMIT = 260
TS_INDEX_LIMIT = 120
BEHAVIOR_MARKERS = [
    "rusqlite::",
    ".execute(",
    ".query_row(",
    "serde_json::from",
    "std::fs::",
    "tokio::",
]


def main() -> int:
    errors: list[str] = []
    for path in sorted((ROOT / "core").glob("*/src/lib.rs")):
        text = path.read_text(encoding="utf-8")
        line_count = len(text.splitlines())
        if line_count > CORE_LIB_LIMIT:
            errors.append(f"{path}: crate root has {line_count} lines; split behavior modules")
        for marker in BEHAVIOR_MARKERS:
            if marker in text:
                errors.append(f"{path}: crate root contains behavior marker {marker!r}")

    for path in sorted((ROOT / "packages").glob("*/src/index.ts")):
        text = path.read_text(encoding="utf-8")
        line_count = len(text.splitlines())
        if line_count > TS_INDEX_LIMIT:
            errors.append(f"{path}: package entry point has {line_count} lines; keep it a facade")
        forbidden = ["class ", "fetch(", "JSON.parse("]
        if path.as_posix().endswith("packages/contracts/src/index.ts"):
            forbidden.append("async ")
        else:
            forbidden.append("function ")
        for marker in forbidden:
            if marker in text:
                errors.append(f"{path}: package entry point contains behavior marker {marker!r}")

    if errors:
        print("architecture guard check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("architecture guard check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
