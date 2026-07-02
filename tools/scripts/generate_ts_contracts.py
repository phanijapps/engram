#!/usr/bin/env python3
"""Generate TypeScript schema constants from the accepted Engram v1 contract."""

from __future__ import annotations

import json
from pathlib import Path


def repo_root() -> Path:
    for path in Path(__file__).resolve().parents:
        if (path / "Cargo.toml").is_file() and (path / "package.json").is_file():
            return path
    raise RuntimeError("could not find repository root")


ROOT = repo_root()
SOURCE = ROOT / "contracts/v1/schemas/engram-v1.schema.json"
TARGET = ROOT / "packages/contracts/src/generated/schema.generated.ts"
TYPE_SCHEMA_TARGET = ROOT / "packages/contracts/src/generated/types.schema.generated.json"


def main() -> int:
    schema = json.loads(SOURCE.read_text(encoding="utf-8"))
    rendered = json.dumps(schema, indent=2, sort_keys=True)
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text(
        "\n".join(
            [
                "/* Generated from contracts/v1/schemas/engram-v1.schema.json. Do not edit. */",
                f"export const engramV1Schema = {rendered} as const;",
                "export const engramV1Definitions = engramV1Schema.$defs;",
                "export type EngramV1DefinitionName = keyof typeof engramV1Definitions;",
                "",
            ]
        ),
        encoding="utf-8",
    )
    wrapper_schema = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "EngramV1Types",
        "type": "object",
        "additionalProperties": False,
        "required": list(schema["$defs"].keys()),
        "properties": {
            name: {"$ref": f"#/$defs/{name}"} for name in schema["$defs"].keys()
        },
        "$defs": schema["$defs"],
    }
    TYPE_SCHEMA_TARGET.write_text(
        json.dumps(wrapper_schema, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"generated {TARGET.relative_to(ROOT)}")
    print(f"generated {TYPE_SCHEMA_TARGET.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
