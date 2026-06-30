#!/usr/bin/env python3
"""Validate Engram contract artifacts.

This script is intentionally independent from implementation crates. It checks
the accepted v1 JSON Schema package, accepted examples, and invalid examples so
contract drift is visible before implementation work starts.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    from jsonschema import Draft202012Validator
except ImportError as exc:  # pragma: no cover - exercised by developer envs.
    print(
        "python package 'jsonschema' is required. Install with: "
        "python3 -m pip install -r requirements-dev.txt",
        file=sys.stderr,
    )
    raise SystemExit(2) from exc


ROOT = Path(__file__).resolve().parents[1]
BASE_SCHEMA = ROOT / "contracts/v1/schemas/engram-v1.schema.json"
EXAMPLES = ROOT / "contracts/v1/examples"
INVALID_EXAMPLES = EXAMPLES / "invalid"

VALID_EXAMPLE_DEFS = {
    "write-memory-request.json": "WriteMemoryRequest",
    "write-memory-response.json": "WriteMemoryResponse",
    "retrieval-request.json": "RetrievalRequest",
    "context-payload.json": "ContextPayload",
    "forget-request.json": "ForgetRequest",
    "forget-result.json": "ForgetResult",
    "forget-request.delete.json": "ForgetRequest",
    "forget-result.delete.json": "ForgetResult",
    "forget-request.redact.json": "ForgetRequest",
    "forget-result.redact.json": "ForgetResult",
    "forget-request.archive.json": "ForgetRequest",
    "forget-result.archive.json": "ForgetResult",
    "evaluation-fixture.json": "EvaluationFixture",
}

INVALID_EXAMPLE_DEFS = {
    "write-memory-request.missing-scope-tenant.json": "WriteMemoryRequest",
    "write-memory-request.training-export.json": "WriteMemoryRequest",
    "memory-record.missing-status.json": "MemoryRecord",
    "memory-record.missing-provenance-actor.json": "MemoryRecord",
    "retrieval-request.missing-requester.json": "RetrievalRequest",
    "context-payload.redacted-content.json": "ContextPayload",
}

REQUIRED_DEFS = [
    "Actor",
    "Requester",
    "Scope",
    "Policy",
    "Provenance",
    "MemoryRecord",
    "MemoryEvent",
    "RetrievalRequest",
    "ContextPayload",
    "WriteMemoryRequest",
    "WriteMemoryResponse",
    "ForgetRequest",
    "ForgetResult",
    "EvaluationFixture",
]


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise AssertionError(f"{path}: invalid JSON: {exc}") from exc


def schema_for(base: dict[str, Any], definition: str) -> dict[str, Any]:
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": f"#/$defs/{definition}",
        "$defs": base["$defs"],
    }


def validation_errors(base: dict[str, Any], definition: str, instance: Any) -> list[str]:
    validator = Draft202012Validator(schema_for(base, definition))
    errors = []
    for error in sorted(validator.iter_errors(instance), key=lambda err: list(err.path)):
        location = ".".join(str(part) for part in error.path) or "<root>"
        errors.append(f"{location}: {error.message}")
    return errors


def semantic_errors(path: Path, instance: Any) -> list[str]:
    """Checks contract invariants that JSON Schema cannot express cleanly."""

    errors: list[str] = []

    if path.name == "context-payload.redacted-content.json":
        for index, item in enumerate(instance.get("items", [])):
            metadata = item.get("metadata", {})
            if metadata.get("memoryStatus") == "redacted" and item.get("content"):
                errors.append(
                    f"items.{index}.content: redacted memory content must not be returned"
                )

    return errors


def ensure_required_defs(base: dict[str, Any]) -> list[str]:
    missing = [name for name in REQUIRED_DEFS if name not in base.get("$defs", {})]
    if missing:
        return ["missing v1 schema definitions: " + ", ".join(missing)]
    return []


def ensure_no_training_export(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    if re.search(r'"training_export"', text):
        return [f"{path}: training_export is excluded from v1"]
    return []


def main() -> int:
    errors: list[str] = []
    base = load_json(BASE_SCHEMA)

    errors.extend(ensure_required_defs(base))

    for path in sorted((ROOT / "contracts").glob("**/*.json")):
        load_json(path)

    for relative, definition in VALID_EXAMPLE_DEFS.items():
        path = EXAMPLES / relative
        if not path.is_file():
            errors.append(f"{path}: missing valid example")
            continue
        instance = load_json(path)
        for error in validation_errors(base, definition, instance):
            errors.append(f"{path}:{error}")

    for relative, definition in INVALID_EXAMPLE_DEFS.items():
        path = INVALID_EXAMPLES / relative
        if not path.is_file():
            errors.append(f"{path}: missing invalid example")
            continue
        instance = load_json(path)
        schema_errors = validation_errors(base, definition, instance)
        invariant_errors = semantic_errors(path, instance)
        if not schema_errors and not invariant_errors:
            errors.append(f"{path}: invalid example unexpectedly passed")

    for path in [
        ROOT / "contracts/v1/schemas/engram-v1.schema.json",
        ROOT / "docs/domain-data-model.md",
    ]:
        errors.extend(ensure_no_training_export(path))

    if errors:
        print("contract validation failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    print("contract validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
