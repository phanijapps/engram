#!/usr/bin/env python3
"""Scaffold a constitution skeleton into a target repo.

Modes:
  greenfield  --repo PATH [--packs a,b,c]     interview flow: intent charters
  brownfield  --repo PATH [--docs D|F ...]    [--packs ...]  code+docs mining

Creates docs/constitution/{00-INDEX.md, DEVIATIONS.md, DECAY-WINDOWS.md,
registry/constitution-catalogue.json, <pack>/{00-INDEX.md, knowledge.md, rules/}}
from assets/templates. Idempotent: refuses to overwrite an existing pack.
Stdlib only — portable to any repo.
"""
import argparse, datetime, json, sys
from pathlib import Path

SKILL = Path(__file__).resolve().parents[1]
TPL = SKILL / "assets/templates"
DEFAULT_PACKS = ["product", "architecture", "dev", "testing", "ci", "security", "release"]
PREFIX = {"product": "PRD", "architecture": "ARC", "dev": "DEV", "testing": "TST",
          "ci": "CIE", "security": "SEC", "release": "REL"}
CHARTER = {
 "product": "What this software promises: scope, observable outcomes, what it is NOT.",
 "architecture": "How it is structured: parts, seams, dependencies, boundaries.",
 "dev": "How artifacts are made: toolchain, builds, reproducibility.",
 "testing": "What proves it: fixtures, coverage discipline, merge safety.",
 "ci": "Enforcement view: which gate enforces which rule on every push. Owns no other pack's rules.",
 "security": "Trust boundaries: what is protected, from whom, and how it is audited.",
 "release": "Shipping: versioning, compatibility, publication, rollback.",
}

def fill(tpl: str, subs: dict) -> str:
    for k, v in subs.items(): tpl = tpl.replace("{{" + k + "}}", v)
    return tpl

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--mode", choices=["greenfield", "brownfield"], required=True)
    ap.add_argument("--packs", default=",".join(DEFAULT_PACKS))
    ap.add_argument("--docs", nargs="*", default=[], help="brownfield: doc files/dirs to mine")
    a = ap.parse_args()

    repo = Path(a.repo).resolve()
    C = repo / "docs" / "constitution"
    if C.exists() and (C / "00-INDEX.md").is_file():
        print(f"engram-constitution: {C} already exists — refusing to overwrite", file=sys.stderr)
        return 2
    packs = [p.strip() for p in a.packs.split(",") if p.strip()]
    for p in packs:
        if p not in PREFIX:
            print(f"unknown pack {p} (known: {', '.join(DEFAULT_PACKS)})", file=sys.stderr); return 2

    date = datetime.date.today().isoformat()
    rows = "\n".join(f"| {p} | {CHARTER[p]} | `{p}/rules/` |" for p in packs)
    (C / "registry").mkdir(parents=True, exist_ok=True)
    (C / "00-INDEX.md").write_text(fill((TPL/"root-INDEX.template.md").read_text(),
        {"REPO": repo.name or "project", "DATE": date, "MODE": a.mode,
         "ONE_PAGE": "<greenfield: from the interview. brownfield: mine the docs and code.>",
         "UNIVERSAL_RULES": "<3-6 rules every pack refines — BCP 14.>",
         "PACK_ROWS": rows}))
    for name, tmpl, subs in (
        ("DEVIATIONS.md", None, None), ("DECAY-WINDOWS.md", None, None)):
        (C/name).write_text(f"# {name.split('.')[0]}\n\n## Active\n\n<!-- keep empty -->\n\n## Historical\n\n| Rule | Scope | Owner | Opened | Closed |\n|---|---|---|---|---|\n")
    (C/"registry/constitution-catalogue.json").write_text(fill((TPL/"catalogue.template.json").read_text(), {"DATE": date, "MODE": a.mode}))
    for p in packs:
        pd = C / p; (pd / "rules").mkdir(parents=True)
        (pd/"00-INDEX.md").write_text(fill((TPL/"pack-INDEX.template.md").read_text(),
            {"PACK": p, "PREFIX": PREFIX[p], "CHARTER": CHARTER[p], "RULE_ROWS": "<none drafted yet>"}))
        (pd/"knowledge.md").write_text(fill((TPL/"knowledge.template.md").read_text(),
            {"PACK": p, "KNOWLEDGE": f"<{a.mode}: from {'interview' if a.mode=='greenfield' else 'code+docs mining'}>"}))
        # seed one DRAFT rule per pack (greenfield placeholder; brownfield replaces during mining)
        seed = f"{PREFIX[p]}-001"
        (pd/"rules"/f"{seed}.md").write_text(fill((TPL/"rule.template.md").read_text(),
            {"RULE_ID": seed, "PACK": p,
             "STATEMENT": "DRAFT — replace with the first real rule for this pack (When X, the system SHALL Y).",
             "REFS": "governs: root universal rules (fill in)", "EVIDENCE": "DRAFT — no evidence yet" if a.mode=="greenfield" else "TBD — mine from code/docs",
             "COUNTEREXAMPLE": "DRAFT — name the violating change", "GATE": "review"}))
    print(f"engram-constitution: scaffolded {a.mode} constitution at {C} ({len(packs)} packs)")
    if a.docs:
        print(f"docs to mine: {a.docs}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
