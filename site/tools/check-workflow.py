#!/usr/bin/env python3
"""Invariant linter for .github/workflows/pages.yml (plan step 3, TEST-003).

Stdlib-only string assertions over the workflow file's own text: pin set,
permission triple, secretlessness, trigger scoping and gate ordering. Exit 0
means every invariant holds; any violation prints a checkable message and
exits nonzero. Run from the repository root: python3 site/tools/check-workflow.py
"""
import re
import sys
from pathlib import Path

WORKFLOW = Path(".github/workflows/pages.yml")


def fail(msg: str) -> None:
    print(f"check-workflow: FAIL {msg}")
    sys.exit(1)


def require(cond: bool, msg: str) -> None:
    if not cond:
        fail(msg)


def main() -> None:
    if not WORKFLOW.is_file():
        fail(f"{WORKFLOW} not found")
    text = WORKFLOW.read_text(encoding="utf-8")

    # Trigger scoping: main pushes limited to site/** + this workflow, plus manual dispatch.
    require(re.search(r"^on:", text, re.M), "missing `on:` trigger block")
    require(re.search(r"^  push:\s*$", text, re.M), "missing push trigger")
    require(re.search(r"^      - main\s*$", text, re.M), "push trigger not scoped to main")
    require('"site/**"' in text, "push paths filter missing site/**")
    require('".github/workflows/pages.yml"' in text, "push paths filter missing the workflow file itself")
    require(re.search(r"^  workflow_dispatch:\s*$", text, re.M), "missing workflow_dispatch")

    # Permission triple: exactly contents:read, pages:write, id-token:write.
    perm = re.search(r"^permissions:\n((?:  [A-Za-z-]+: [a-z]+\n)+)", text, re.M)
    require(perm is not None, "missing top-level permissions block")
    granted = dict(
        line.strip().split(": ")
        for line in perm.group(1).strip().splitlines()
    )
    require(
        granted == {"contents": "read", "pages": "write", "id-token": "write"},
        f"permissions must be exactly contents:read pages:write id-token:write, got {granted}",
    )

    # Secretless: no secrets context anywhere.
    require("secrets." not in text, "workflow references the secrets context (REQ-009 forbids it)")

    # Concurrency: group pages, never cancel-in-progress.
    conc = re.search(r"^concurrency:\n  group: (\S+)\n  cancel-in-progress: (\S+)", text, re.M)
    require(conc is not None, "missing concurrency block")
    require(conc.group(1) == "pages", f"concurrency group must be pages, got {conc.group(1)}")
    require(conc.group(2) == "false", "cancel-in-progress must be false")

    # Pinned official actions, exactly the house set.
    uses = re.findall(r"uses: (\S+)", text)
    expected = {
        "actions/checkout@v4",
        "actions/configure-pages@v6",
        "actions/jekyll-build-pages@v1",
        "actions/upload-pages-artifact@v5",
        "actions/deploy-pages@v5",
    }
    require(set(uses) == expected, f"action pins must be exactly {sorted(expected)}, got {sorted(set(uses))}")

    # Build source is ./site only (never docs/, never repo root — RISK-007).
    jekyll = re.search(r"uses: actions/jekyll-build-pages@v1\n\s+with:\n\s+source: (\S+)\n\s+destination: (\S+)", text)
    require(jekyll is not None, "jekyll-build-pages step missing source/destination inputs")
    require(jekyll.group(1) == "./site", f"build source must be ./site, got {jekyll.group(1)}")
    require(jekyll.group(2) == "./site/_site", f"build destination must be ./site/_site, got {jekyll.group(2)}")

    # Gates before upload: link and contrast gates precede the artifact upload.
    link_gate = text.find("site/tools/check-links.py")
    contrast_gate = text.find("site/tools/check-contrast.py")
    upload = text.find("actions/upload-pages-artifact@v5")
    deploy = text.find("actions/deploy-pages@v5")
    require(link_gate != -1, "link gate step missing")
    require(contrast_gate != -1, "contrast gate step missing")
    require(upload != -1, "upload-pages-artifact step missing")
    require(deploy != -1, "deploy-pages step missing")
    require(link_gate < upload, "link gate must run before upload-pages-artifact")
    require(contrast_gate < upload, "contrast gate must run before upload-pages-artifact")
    require(upload < deploy, "upload-pages-artifact must run before deploy-pages")

    # Deploy depends on the gated build job.
    require(re.search(r"^  deploy:\n    needs: build\s*$", text, re.M), "deploy job must need build")

    print("check-workflow: OK — pins, permission triple, secretless, paths filter,")
    print("concurrency, ./site-only source and gate-before-upload ordering all hold")


if __name__ == "__main__":
    main()
