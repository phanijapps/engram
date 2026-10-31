#!/usr/bin/env python3
"""Surface-parity gate (ADR-0022 / constitution rail 02 / SP-001, SP-002).

Ported 1:1 from check-surface-parity.sh (2026-10-31 python consolidation).
Asserts every provider `require_*` capability on the Rust facade is reachable
from the N-API binding: a `require*Api` proxy, a documented special case, or
acknowledged debt.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT, fail, ok  # noqa: E402

NAME = "surface-parity"
FACADE = ROOT / "core/integration/src/provider.rs"
BINDING = ROOT / "bindings/node/src/provider.rs"

SPECIAL_CASES = [
    "knowledge=>requireGraphApi (NativeGraphApi holds knowledge + graph)",
    "consolidation=>consolidateJson (direct method on NativeProvider)",
]
DEBT_ALLOWLIST: list[str] = []  # shrink by adding proxies, then removing entries


def to_camel(snake: str) -> str:
    return "".join(w[:1].upper() + w[1:] for w in snake.split("_"))


def main() -> int:
    if not FACADE.is_file():
        print(f"{NAME}: facade not found: {FACADE}", file=sys.stderr)
        return 2
    if not BINDING.is_file():
        print(f"{NAME}: binding not found: {BINDING}", file=sys.stderr)
        return 2

    facade_requires = sorted(
        set(re.findall(r"pub fn (require_[a-z_]+)", FACADE.read_text()))
    )
    binding_proxies = set(
        re.findall(r"\brequire[A-Za-z]+Api\b", BINDING.read_text())
    )

    missing = []
    for req in facade_requires:
        cap = req[len("require_"):]
        if any(sc.startswith(f"{cap}=>") for sc in SPECIAL_CASES):
            continue
        if cap in DEBT_ALLOWLIST:
            continue
        proxy = f"require{to_camel(cap)}Api"
        if proxy not in binding_proxies:
            missing.append(f"{cap} (expected {proxy})")

    if missing:
        print(f"{NAME}: require_* on facade with no N-API proxy:", file=sys.stderr)
        return fail(NAME, missing)
    return ok(
        NAME,
        f"{len(facade_requires)} capabilities: proxied + {len(SPECIAL_CASES)} special "
        f"+ {len(DEBT_ALLOWLIST)} acknowledged-debt",
    )


if __name__ == "__main__":
    sys.exit(main())
