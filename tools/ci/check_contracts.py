#!/usr/bin/env python3
"""Contracts gate (CI-004 / PR-001).

Ported from check-contracts.sh (2026-10-31 python consolidation): required
contract files, required domain-model sections, JSON validity (stdlib json —
no jq dependency), then validate_contracts.py for schema-level checks.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT  # noqa: E402

os.chdir(ROOT)
NAME = "contracts"

REQUIRED_FILES = [
    "docs/constitution/architecture/domain-data-model.md",
    "docs/constitution/architecture/00-INDEX.md",
    "docs/constitution/registry/contracts/schemas/memory-record.schema.json",
    "docs/constitution/registry/contracts/schemas/memory-query.schema.json",
    "docs/constitution/registry/contracts/v1/README.md",
    "docs/constitution/registry/contracts/v1/acceptance.md",
    "docs/constitution/registry/contracts/v1/compatibility.md",
    "docs/constitution/registry/contracts/v1/changelog.md",
    "docs/constitution/registry/contracts/v1/review-checklist.md",
    "docs/constitution/registry/contracts/v1/schemas/engram-v1.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/memory-record.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/retrieval-request.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/context-payload.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/write-memory-request.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/write-memory-response.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/forget-request.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/forget-result.schema.json",
    "docs/constitution/registry/contracts/v1/schemas/evaluation-fixture.schema.json",
    "docs/constitution/registry/contracts/v1/examples/write-memory-request.json",
    "docs/constitution/registry/contracts/v1/examples/write-memory-response.json",
    "docs/constitution/registry/contracts/v1/examples/retrieval-request.json",
    "docs/constitution/registry/contracts/v1/examples/context-payload.json",
    "docs/constitution/registry/contracts/v1/examples/forget-request.json",
    "docs/constitution/registry/contracts/v1/examples/forget-result.json",
    "docs/constitution/registry/contracts/v1/examples/evaluation-fixture.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/write-memory-request.missing-scope-tenant.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/write-memory-request.training-export.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/memory-record.missing-status.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/memory-record.missing-provenance-actor.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/retrieval-request.missing-requester.json",
    "docs/constitution/registry/contracts/v1/examples/invalid/context-payload.redacted-content.json",
    "docs/constitution/registry/contracts/v1/acceptance/memory-fixtures.md",
    "docs/constitution/registry/contracts/v1/acceptance/retrieval-fixtures.md",
    "docs/constitution/registry/contracts/v1/acceptance/forget-modes.md",
]

REQUIRED_SECTIONS = [
    "## Contract Freeze Policy",
    "## Memory Model",
    "## Belief Network Model",
    "## Knowledge Model",
    "## Hierarchy Model",
    "## Retrieval Model",
    "## Operation Payloads",
    "## Invariants",
    "## V1 Acceptance Decisions",
    "## Deferred Questions For Extension Contracts",
]

FORBIDDEN_EXAMPLE = '"training_export"'


def main() -> int:
    for f in REQUIRED_FILES:
        if not Path(f).is_file():
            print(f"{NAME}: missing required contract file: {f}", file=sys.stderr)
            return 1

    domain = Path("docs/constitution/architecture/domain-data-model.md").read_text()
    for section in REQUIRED_SECTIONS:
        if section not in domain:
            print(f"{NAME}: missing required domain model section: {section}", file=sys.stderr)
            return 1

    for jf in sorted(Path("docs/constitution/registry/contracts/v1").rglob("*.json")):
        try:
            json.loads(jf.read_text())
        except json.JSONDecodeError as e:
            print(f"{NAME}: invalid JSON {jf}: {e}", file=sys.stderr)
            return 1

    # deny drift: forbidden payload field must not reappear in schemas/model
    targets = [Path("docs/constitution/registry/contracts/v1/schemas"), Path("docs/constitution/architecture/domain-data-model.md")]
    for target in targets:
        if target.is_dir():
            found = any(FORBIDDEN_EXAMPLE in f.read_text(errors="replace") for f in target.rglob("*.json"))
        else:
            found = FORBIDDEN_EXAMPLE in target.read_text()
        if found:
            print(f"{NAME}: forbidden field {FORBIDDEN_EXAMPLE} present in {target}", file=sys.stderr)
            return 1

    rc = subprocess.run(
        ["python3", str(ROOT / "tools/contracts/validate_contracts.py")]
    ).returncode
    if rc != 0:
        return rc

    print(f"{NAME}: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
