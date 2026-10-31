#!/usr/bin/env python3
"""Documentation umbrella gate (PC-003, OR-003).

Ported from check-docs.sh (2026-10-31 python consolidation). This is the
orchestrator: it IMPORTS the three checker modules living beside it (no more
wrapper-in-scripts-ci / impl-in-tools split) and runs their main() in order.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT, git_tracked, report  # noqa: E402

os.chdir(ROOT)
NAME = "docs"

# 1. placeholder markers in tracked docs/contracts/skills (*.upstream.md exempt)
MARKER = re.compile(r"TO""DO|\[TO""DO")
CODE = re.compile(r"`[^`]*`|<!--.*?-->")  # inline code + HTML comments are citations, not placeholders
tracked = []
for pattern in ("README.md", "AGENTS.md", "docs/**", "docs/contracts/**", ".codex/skills/**"):
    tracked += git_tracked(pattern)
# *.upstream.md are frozen upstream snapshots; docs/research/ legitimately
# QUOTES markers (decay-window baselines, gate documentation) — quoting is not
# an unresolved placeholder.
marker_paths = [
    p for p in dict.fromkeys(tracked)
    if not p.endswith(".upstream.md") and not p.startswith("docs/research/") and Path(p).is_file()
]
hits = []
for p in marker_paths:
    if MARKER.search(CODE.sub("", Path(p).read_text(errors="replace"))):
        hits.append(p)
if hits:
    print(f"{NAME}: unresolved documentation/skill placeholder markers:", file=sys.stderr)
    for h in hits:
        print(f"  {h}", file=sys.stderr)
    sys.exit(1)

# 2. tracked skill validation (system validator when present)
validator = Path.home() / ".codex/skills/.system/skill-creator/scripts/quick_validate.py"
if validator.is_file():
    skill_dirs = sorted({str(Path(p).parent) for p in git_tracked(".codex/skills/**/SKILL.md")})
    for skill in skill_dirs:
        r = subprocess.run(["python3", str(validator), skill], capture_output=True)
        if r.returncode != 0:
            print(f"{NAME}: skill validation failed for {skill}", file=sys.stderr)
            sys.exit(1)
else:
    print(f"{NAME}: warning: skill validator not found: {validator}", file=sys.stderr)

# 3. the three sub-checkers, imported (reuse, no subprocess)
import check_architecture_guards  # noqa: E402
import check_ts_native_delegation  # noqa: E402
import check_research_parity_docs  # noqa: E402

for mod in (check_architecture_guards, check_ts_native_delegation, check_research_parity_docs):
    rc = mod.main()
    if rc != 0:
        sys.exit(rc)

# 4. code-docs gate (rust + ts doc coverage, DW-6 advisory rust half)
import check_code_docs  # noqa: E402

sys.exit(check_code_docs.main())
