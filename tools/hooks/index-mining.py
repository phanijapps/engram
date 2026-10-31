#!/usr/bin/env python3
"""Index-mining advisor — reusable hook for all agent harnesses.

Injects the "mine the engram index before judging anything new" instruction
into every agent session on every harness that runs shell hooks. The
instruction uses ONLY the one-shot CLI (plain bash) so it works whether or
not the harness provides MCP tool access to subagents.

Portable: pure-stdlib Python 3.9+, no deps, exits 0 always (fail-open).
Reusable across harnesses:

  Claude Code   .claude/settings.json → SessionStart / PreToolUse hooks
  Codex         .codex/hooks/ or AGENTS.md sourcing
  GitHub Copilot .github/copilot-instructions.md references this script
  Pi            .pi/extensions/ or AGENTS.md (pi reads AGENTS.md)
  Future        any harness that can run `python3 tools/hooks/index-mining.py`

Usage:
  python3 tools/hooks/index-mining.py            # print the instruction
  python3 tools/hooks/index-mining.py --probe    # verify the CLI works
  python3 tools/hooks/index-mining.py --json     # machine-readable envelope

Environment overrides:
  ENGRAM_HOOKS=0        → suppress entirely
  ENGRAM_STORAGE=...    → storage dir (default ~/.engram)
  ENGRAM_PROJECT=...    → project scope (default agentzero)
  ENGRAM_BIN=...        → engram-mcp binary path
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys

VERSION = "1.0.0"


def _binary() -> str:
    """Locate engram-mcp: explicit env → repo-local → PATH."""
    explicit = os.environ.get("ENGRAM_BIN")
    if explicit and pathlib.Path(explicit).exists():
        return explicit
    # Walk up to repo root looking for target/release/engram-mcp
    d = pathlib.Path(__file__).resolve().parent
    for _ in range(6):
        candidate = d / "target" / "release" / "engram-mcp"
        if candidate.exists():
            return str(candidate)
        d = d.parent
    found = shutil.which("engram-mcp")
    return found or ""


def _storage() -> str:
    return os.environ.get("ENGRAM_STORAGE",
                          str(pathlib.Path.home() / ".engram"))


def _project() -> str:
    return os.environ.get("ENGRAM_PROJECT", "agentzero")


def probe() -> dict:
    """Check the one-shot CLI is reachable and the store responds."""
    binary = _binary()
    if not binary:
        return {"ok": False, "reason": "engram-mcp binary not found"}
    try:
        r = subprocess.run(
            [binary, "status", "--storage", _storage(), "--project", _project()],
            capture_output=True, text=True, timeout=10,
        )
        return {"ok": r.returncode == 0, "binary": binary,
                "rc": r.returncode}
    except Exception as e:
        return {"ok": False, "reason": str(e)}


INSTRUCTION = """\
## Index mining (MANDATORY before judging anything new)

This repository has a code+memory index (engram). Query it via one-shot CLI
(plain bash — no MCP needed) BEFORE authoring any artifact:

  # Memory: past patterns, past failures, decisions, lessons
  {bin} recall --query "<past-pattern terms>" --json --limit 5 --storage {storage} --project {project}

  # Code: symbol search, cross-file callers
  {bin} search --query "<symbol_name>" --storage {storage} --project {project}

  # Code index: bound branch + indexed revision
  {bin} code --op status --repo <repo-id> --json --storage {storage} --project {project}

Completion criteria: cite ≥2 index results in your artifact (or explicitly
state "index empty for <query>" if nothing relevant). An artifact with zero
index evidence and no "index empty" statement will be rejected by the
validator. Do not fabricate results — record what the index actually returned.
"""


def instruction() -> str:
    binary = _binary() or "engram-mcp"
    return INSTRUCTION.format(bin=binary, storage=_storage(),
                              project=_project())


def main():
    if os.environ.get("ENGRAM_HOOKS") == "0":
        sys.exit(0)

    if "--probe" in sys.argv:
        result = probe()
        print(json.dumps(result, indent=2))
        sys.exit(0 if result.get("ok") else 1)

    if "--json" in sys.argv:
        print(json.dumps({
            "version": VERSION,
            "instruction": instruction(),
            "probe": probe(),
        }, indent=2))
        sys.exit(0)

    # Default: print the instruction (harnesses embed stdout as context)
    print(instruction())
    sys.exit(0)


if __name__ == "__main__":
    main()
