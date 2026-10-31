#!/usr/bin/env python3
"""Remove generated JS/native build outputs while preserving dependencies,
Cargo artifacts, generated contracts, and local demo databases.

Ported from tools/scripts/clean-workspace.mjs (2026-10-31 python
consolidation). Cross-platform: pure stdlib.
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

PATHS = [
    "demo/backend/dist",
    "demo/frontend/dist",
    "packages/adapters/dist",
    "packages/client/dist",
    "packages/contracts/dist",
    "packages/node/dist",
    "packages/node/engram_node.node",
]


def main() -> int:
    for rel in PATHS:
        target = ROOT / rel
        if target.is_dir():
            shutil.rmtree(target, ignore_errors=True)
        elif target.exists():
            target.unlink()
        print(f"removed {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
