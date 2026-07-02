#!/usr/bin/env python3
"""Update loop-da-loop implementation phase status."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ALLOWED_STATUSES = {"DRAFT", "IN_PROGRESS", "BLOCKED", "DONE"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase_id", help="Phase id such as PHASE04")
    parser.add_argument("status", choices=sorted(ALLOWED_STATUSES))
    parser.add_argument(
        "--file",
        default="docs/implementation/phases.json",
        help="Phase JSON file to update",
    )
    args = parser.parse_args()

    path = Path(args.file)
    phases = json.loads(path.read_text(encoding="utf-8"))
    for phase in phases:
        if phase["phase_id"] == args.phase_id:
            phase["status"] = args.status
            path.write_text(json.dumps(phases, indent=2) + "\n", encoding="utf-8")
            return

    raise SystemExit(f"phase not found: {args.phase_id}")


if __name__ == "__main__":
    main()
