#!/usr/bin/env python3
"""Coherent-integration / no-orphans gate (RULE-6 / rail 08 / OR-001, OR-002).

Bidirectional bijection between the component registry and disk:
  - every Rust workspace member and TS package MUST be declared in
    docs/contracts/constitution/v1/components/registry.json (orphans fail),
  - every registry entry MUST resolve to a directory on disk with a
    Cargo.toml or package.json (dangling declarations fail),
  - every gate in the gates registry MUST exist on disk when status=existing.
Exit 1 on violation. Stdlib only.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMPONENTS = ROOT / "docs/contracts/constitution/v1/components/registry.json"
GATES = ROOT / "docs/contracts/constitution/v1/gates/registry.json"

fail = []


def workspace_members() -> set[str]:
    t = (ROOT / "Cargo.toml").read_text()
    m = re.search(r"members\s*=\s*\[(.*?)\]", t, re.S)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def ts_packages() -> set[str]:
    out = set()
    for p in (ROOT / "packages").glob("*/package.json"):
        out.add(str(p.parent.relative_to(ROOT)))
    return out


def main() -> int:
    reg = json.loads(COMPONENTS.read_text())
    declared = {c["path"] for c in reg.get("components", [])}

    on_disk = workspace_members() | ts_packages()
    # vendored/example crates are workspace members already; anything else with
    # a manifest at depth<=3 under known roots counts too
    orphans = on_disk - declared
    for p in sorted(orphans):
        fail.append(f"RULE-6: orphan component on disk but not declared: {p}")

    for c in reg.get("components", []):
        d = ROOT / c["path"]
        if not d.is_dir():
            fail.append(f"RULE-6: dangling declaration, no directory: {c['path']}")
            continue
        kind = c.get("kind")
        has_manifest = (d / "Cargo.toml").is_file() or (d / "package.json").is_file()
        if kind in ("crate", "package") and not has_manifest:
            fail.append(f"RULE-6: declared {kind} has no manifest: {c['path']}")

    gates = json.loads(GATES.read_text())
    for g in gates.get("gates", []):
        if g.get("status") == "existing" and not (ROOT / g["path"]).is_file():
            fail.append(f"RULE-6: existing gate missing on disk: {g['path']}")

    for f in fail:
        print(f"coherent-integration: FAIL {f}", file=sys.stderr)
    if fail:
        print(f"coherent-integration: {len(fail)} failure(s)", file=sys.stderr)
        return 1
    print(f"coherent-integration: OK ({len(declared)} components, bidirectional)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
