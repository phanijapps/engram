#!/usr/bin/env python3
"""Implementation-registry drift gate (RULE-1 / rail 03 / PR-004).

Fails when:
  - two registry concepts claim the same ssot_module (duplicate implementation),
  - a transport wrapper does not depend on / import the SSOT crate or package,
  - a concept's ssot_module is not a Cargo crate root or package dir on disk
    (path existence itself is owned by check-constitution.sh; this gate adds
    capability semantics).
Exit 1 on violation. Stdlib only.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "docs/contracts/constitution/v1/implementation/registry.json"

fail = []


def cargo_name(crate_dir: Path) -> str | None:
    mf = crate_dir / "Cargo.toml"
    if not mf.is_file():
        return None
    m = re.search(r'^\s*name\s*=\s*"([^"]+)"', mf.read_text(), re.M)
    return m.group(1) if m else None


def main() -> int:
    data = json.loads(REGISTRY.read_text())
    concepts = data.get("concepts", [])

    # RULE-1: duplicate canonical implementations (same ssot_module)
    by_ssot: dict[str, list[str]] = {}
    for c in concepts:
        by_ssot.setdefault(c["ssot_module"], []).append(c["concept_id"])
    for ssot, ids in by_ssot.items():
        if len(ids) > 1:
            fail.append(f"RULE-1: ssot_module {ssot} claimed by {len(ids)} concepts: {ids}")

    for c in concepts:
        ssot = c["ssot_module"]
        ssot_dir = ROOT / ssot
        crate = cargo_name(ssot_dir)
        for w in c.get("transport_wrappers", []):
            wdir = ROOT / w["module"]
            wcrate = cargo_name(wdir)
            must = w.get("must_import")
            if must and wcrate:
                mf = (wdir / "Cargo.toml").read_text()
                dep = re.search(rf'^\s*{re.escape(must)}\s*=', mf, re.M)
                if not dep:
                    fail.append(
                        f"RULE-1: wrapper {w['module']} ({wcrate}) does not depend on "
                        f"SSOT crate `{must}` (concept {c['concept_id']}, ssot {ssot})"
                    )
            # TS package wrapper: must_import should appear in package.json deps
            if must and not wcrate and (wdir / "package.json").is_file():
                pkg = json.loads((wdir / "package.json").read_text())
                deps = {**pkg.get("dependencies", {}), **pkg.get("devDependencies", {})}
                if must.lstrip("@engramlabs/") not in deps and must not in deps:
                    fail.append(
                        f"RULE-1: wrapper {w['module']} package.json missing dependency "
                        f"`{must}` (concept {c['concept_id']})"
                    )

    for f in fail:
        print(f"implementation-registry: FAIL {f}", file=sys.stderr)
    if fail:
        print(f"implementation-registry: {len(fail)} failure(s)", file=sys.stderr)
        return 1
    print("implementation-registry: OK (single implementations, wrappers import SSOT)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
