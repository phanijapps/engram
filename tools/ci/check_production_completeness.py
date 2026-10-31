#!/usr/bin/env python3
"""Production-completeness gate (RULE-3 / rail 05 / PR-002).

Fails when:
  - shipped first-party Rust code (core/, adapters/, bindings/, backends/,
    mcp/, codegraph/) contains TODO/FIXME/XXX/todo!/unimplemented! markers
    (baseline measured 2026-10-31: 0 — enforced near-strict from day one),
  - any core/* or adapters/* Cargo.toml depends on an LLM/model SDK crate
    (PR-002: LLM calls stay in TypeScript; the Rust core stays deterministic).
Exit 1 on violation. Stdlib only.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SHIPPED = ["core", "adapters", "bindings", "backends", "mcp", "codegraph"]
MARKER = re.compile(r"\b(TODO|FIXME|XXX)\b|todo!|unimplemented!")
LLM_DEPS = re.compile(
    r"(openai|anthropic|llm|cohere|mistralai|llama|genai|dashscope|deepseek|zhipu)", re.I
)

fail = []


def main() -> int:
    markers = 0
    for top in SHIPPED:
        base = ROOT / top
        if not base.is_dir():
            continue
        for f in base.rglob("*"):
            if not f.is_file() or f.suffix not in (".rs", ".ts", ".tsx"):
                continue
            if "target" in f.parts or "node_modules" in f.parts:
                continue
            try:
                text = f.read_text(errors="replace")
            except OSError:
                continue
            hits = MARKER.findall(text)
            if hits:
                markers += len(hits)
                rel = f.relative_to(ROOT)
                fail.append(f"RULE-3: stub marker in shipped code: {rel}")

    # PR-002 dependency lint: no LLM SDKs in core/adapters manifests
    for top in ("core", "adapters"):
        for mf in (ROOT / top).rglob("Cargo.toml"):
            body = mf.read_text()
            m = re.search(r"\[dependencies\](.*?)(\n\[|\Z)", body, re.S)
            if not m:
                continue
            for dep in re.findall(r"^([A-Za-z0-9_-]+)\s*=", m.group(1), re.M):
                if LLM_DEPS.fullmatch(dep) or LLM_DEPS.search(dep) and "-" in dep:
                    fail.append(
                        f"PR-002: LLM/model SDK dependency `{dep}` in {mf.relative_to(ROOT)}"
                    )

    for f in fail[:25]:
        print(f"production-completeness: FAIL {f}", file=sys.stderr)
    if len(fail) > 25:
        print(f"production-completeness: ... and {len(fail)-25} more", file=sys.stderr)
    if fail:
        return 1
    print("production-completeness: OK (0 markers; no LLM SDKs in core/adapters)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
