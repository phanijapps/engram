#!/usr/bin/env python3
"""Code-docs gate (module + public item doc coverage).

Ported from check-code-docs.sh (2026-10-31 python consolidation). Rust half
retargeted to real workspace roots (DW-6): ADVISORY until the decay window
closes (baseline 77 issues + 30 rustdoc errors); `CHECK_CODE_DOCS_STRICT=1`
enforces now. The TS half is blocking, as before.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT  # noqa: E402

os.chdir(ROOT)
NAME = "code-docs"
STRICT = os.environ.get("CHECK_CODE_DOCS_STRICT", "") == "1"

RUST_ROOTS = ["core", "adapters", "backends", "bindings", "codegraph", "mcp"]


def rust_check() -> list[str]:
    errors: list[str] = []
    files: list[Path] = []
    for r in RUST_ROOTS:
        base = Path(r)
        if base.is_dir():
            files += [p for p in base.rglob("*/src/**/*.rs") if "target" not in p.parts]
    for path in sorted(files):
        lines = path.read_text(errors="replace").splitlines()
        first = next((l.strip() for l in lines if l.strip()), "")
        if not first.startswith("//!"):
            errors.append(f"{path}: missing module docs at top of file")
            continue
        doc = " ".join(
            l.strip().removeprefix("//!").strip() for l in lines if l.strip().startswith("//!")
        ).strip()
        if len(doc) < 120:
            errors.append(f"{path}: module docs are too thin; explain ownership and boundaries")
    return errors


def ts_check() -> list[str]:
    import re

    errors: list[str] = []
    export_re = re.compile(r"export\s+(async\s+)?(function|class|interface|type)\s+")
    ts_files = [
        p for p in sorted(Path("packages").glob("*/src/**/*"))
        if p.suffix in {".ts", ".tsx"} and not p.name.endswith(".generated.ts")
    ]
    for path in ts_files:
        lines = path.read_text(errors="replace").splitlines()
        for i, line in enumerate(lines):
            stripped = line.strip()
            if not export_re.match(stripped):
                continue
            j = i - 1
            while j >= 0 and (not lines[j].strip() or lines[j].strip().startswith("//")):
                j -= 1
            prev = lines[j].strip() if j >= 0 else ""
            if not (prev.startswith("/**") or prev.startswith("*")):
                errors.append(f"{path}:{i + 1}: exported TypeScript surface missing JSDoc")
    return errors


def main() -> int:
    ts_errors = ts_check()
    if ts_errors:
        print(f"{NAME}: TypeScript documentation review failed:", file=sys.stderr)
        for e in ts_errors:
            print(f"  {e}", file=sys.stderr)
        return 1

    rust_errors = rust_check()
    if rust_errors and STRICT:
        print(f"{NAME}: code documentation review failed (strict):", file=sys.stderr)
        for e in rust_errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    if rust_errors:
        print(
            f"{NAME}: ADVISORY (DW-6) — {len(rust_errors)} rust doc issue(s); strict at window close",
            file=sys.stderr,
        )

    if STRICT:
        env = {**os.environ, "RUSTDOCFLAGS": "-D warnings"}
        return subprocess.run(
            ["cargo", "doc", "--workspace", "--no-deps", "--document-private-items"],
            env=env, stdout=subprocess.DEVNULL,
        ).returncode
    print(
        f"{NAME}: ADVISORY (DW-6) — rustdoc -D warnings skipped; baseline 30 errors / 5 crates",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
