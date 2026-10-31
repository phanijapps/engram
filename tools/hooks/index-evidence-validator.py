#!/usr/bin/env python3
"""Index-evidence validator — reusable post-artifact gate.

Checks that analysis.md and plan.md contain evidence of engram index
consultation: either cited index results or an explicit "index empty"
statement. Integrated into factory.py validate-artifact as an additional
check, but also runnable standalone.

Portable: pure-stdlib Python 3.9+, exits 0 always (advisory mode) or
non-zero (--strict mode for CI/factory gates).
"""
import re
import sys
import pathlib

# Artifacts that REQUIRE index evidence
REQUIRES_EVIDENCE = {"analysis.md", "plan.md"}

# Patterns that count as index-evidence citations
EVIDENCE_PATTERNS = [
    r"recall\s*--query",           # CLI recall invocation
    r"search\s*--query",           # CLI search invocation
    r"symbol_context",             # MCP tool name
    r"engram_recall",              # MCP namespace form
    r"engram_search",              # MCP namespace form
    r"fetch_content",              # MCP tool for exact reads
    r"index empty for",            # explicit empty statement
    r"index.*no results",          # alternative empty statement
    r"\[proven\].*recall",         # proven-evidence label citing recall
    r"prior art.*from.*index",     # prior-art section citing index
    r"Past pattern.*recall",       # past-pattern citing recall
]


def has_index_evidence(artifact_path: str) -> tuple[bool, str]:
    """Returns (has_evidence, detail)."""
    path = pathlib.Path(artifact_path)
    if not path.exists():
        return False, "artifact not found"

    name = path.name.lower()
    if name not in REQUIRES_EVIDENCE:
        return True, "artifact does not require index evidence"

    text = path.read_text(encoding="utf-8", errors="replace")

    for pattern in EVIDENCE_PATTERNS:
        match = re.search(pattern, text, re.IGNORECASE)
        if match:
            line_no = text[:match.start()].count("\n") + 1
            return True, f"evidence found at line {line_no}: {match.group()}"

    return False, (
        f"no index-evidence citation found in {name}. "
        f"Add a 'Prior art' section citing ≥2 engram index results "
        f"(recall/search invocations or results), or state "
        f"'index empty for <query>' if nothing relevant was found."
    )


def main():
    if len(sys.argv) < 2:
        print("usage: index-evidence-validator.py <artifact> [--strict]")
        sys.exit(2)

    artifact = sys.argv[1]
    strict = "--strict" in sys.argv
    has, detail = has_index_evidence(artifact)

    prefix = "PASS" if has else ("FAIL" if strict else "WARN")
    print(f"index-evidence: {prefix} — {detail}")
    sys.exit(0 if has or not strict else 1)


if __name__ == "__main__":
    main()
