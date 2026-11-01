#!/usr/bin/env python3
"""Constitution umbrella gate (RULE-5 / rail 07 + L1 structural). Ported from check-constitution.sh."""

from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT  # noqa: E402

os.chdir(ROOT)

import json
import re

fail, warn = [], []
def F(msg): fail.append(msg)
def W(msg): warn.append(msg)

# --- L1 structural: required artifacts ---
required = [
    "docs/constitution/00-INDEX.md",
    "docs/constitution/DEVIATIONS.md",
    "docs/constitution/DECAY-WINDOWS.md",
    "docs/contracts/constitution/v1/implementation/registry.json",
    "docs/contracts/constitution/v1/schemas/software-constitution-implementation-registry.v1.json",
]
for p in required:
    if not Path(p).is_file(): F(f"missing required artifact: {p}")

rails = sorted(Path("docs/constitution").glob("0[1-9]-*.md")) if Path("docs/constitution").is_dir() else []
if len(rails) < 1: F("no numbered constitution rails found")
index = Path("docs/constitution/00-INDEX.md").read_text() if Path("docs/constitution/00-INDEX.md").is_file() else ""
for rail in rails:
    if rail.name not in index:
        F(f"rail {rail.name} not referenced in 00-INDEX.md")

# --- RULE-5: zero deviations ---
dev = Path("docs/constitution/DEVIATIONS.md")
if dev.is_file():
    text = dev.read_text()
    m = re.search(r"^## Active deviations\s*$(.*?)(?=^## )", text, re.S | re.M)
    active = m.group(1) if m else ""
    # any non-comment, non-empty content under Active is a violation
    substantive = [l for l in active.splitlines()
                   if l.strip() and not l.strip().startswith("<!--") and not l.strip().startswith("| Rule |") and not l.strip().startswith("|---")]
    if substantive:
        F(f"DEVIATIONS.md has {len(substantive)} active line(s) — zero-deviation mandate violated")
    # every active-style deviation row (even historical) needs a closed date
    for row in re.findall(r"^\|(?!\s*Rule)(?!\s*-).*\|", text, re.M):
        if len([c for c in row.split("|") if c.strip()]) >= 6 and "historical" not in text.lower():
            pass  # shape enforced loosely; strict per-row checks once entries exist

# --- Decay windows: format + expiry ---
dw = Path("docs/constitution/DECAY-WINDOWS.md")
if dw.is_file():
    import datetime
    text = dw.read_text()
    active_text = text.split("## Closure record")[0]
    for row in re.findall(r"^\| DW-\d+.*\|$", active_text, re.M):
        cols = [c.strip() for c in row.strip("|").split("|")]
        if len(cols) != 6: F(f"decay window row has {len(cols)} columns (need 6): {row[:60]}")
        else:
            m = re.match(r"(\d{4}-\d{2}-\d{2})$", cols[5])
            if m:
                d = datetime.date.fromisoformat(cols[5])
                closure_text = text.split("## Closure record")[1] if "## Closure record" in text else ""
                if d < datetime.date.today() and cols[0] not in closure_text:
                    F(f"decay window {cols[0]} expired {cols[5]} without closure record")

# --- Registries: parse + structural validation + path existence ---
def load(p):
    try:
        return json.loads(Path(p).read_text())
    except Exception as e:
        F(f"registry {p} does not parse: {e}"); return {}

impl = load("docs/contracts/constitution/v1/implementation/registry.json")
if impl:
    if impl.get("schema") != "software-constitution.implementation_registry.v1":
        F("implementation registry: wrong schema field")
    ids = [c.get("concept_id") for c in impl.get("concepts", [])]
    if len(ids) != len(set(ids)): F("implementation registry: duplicate concept_id")
    for c in impl.get("concepts", []):
        for k in ("concept_id", "name", "ssot_module"):
            if not c.get(k): F(f"implementation registry concept missing {k}: {c.get('concept_id')}")
        if not re.match(r"^[a-z][a-z0-9._-]*$", c.get("concept_id", "")):
            F(f"implementation registry: bad concept_id {c.get('concept_id')}")
        if not Path(c["ssot_module"]).exists():
            F(f"implementation registry: ssot_module path missing: {c['ssot_module']}")
        for w in c.get("transport_wrappers", []):
            if not Path(w.get("module", "")).exists():
                F(f"transport wrapper path missing: {w.get('module')} (concept {c.get('concept_id')})")

gates = load("docs/contracts/constitution/v1/gates/registry.json")
classes_covered = set()
for g in gates.get("gates", []):
    if g.get("status") == "existing":
        if not Path(g["path"]).is_file(): F(f"gates registry: existing gate missing on disk: {g['path']}")
        if g.get("blocking"): classes_covered.add(g.get("rule_class"))
    elif g.get("status") == "planned":
        if Path(g["path"]).is_file(): W(f"gates registry: gate marked planned but exists on disk — flip status: {g['path']}")
for rc in ("RULE-1", "RULE-3", "RULE-5"):
    if rc not in classes_covered:
        F(f"no blocking existing gate for {rc} (minimum L1 discipline set)")

comps = load("docs/contracts/constitution/v1/components/registry.json")
if comps:
    paths = [c["path"] for c in comps.get("components", [])]
    if len(paths) != len(set(paths)): F("components registry: duplicate path")
    for c in comps.get("components", []):
        if not Path(c["path"]).exists(): F(f"components registry: declared path missing on disk: {c['path']}")

events = load("docs/contracts/constitution/v1/events/registry.json")
for fam in events.get("families", []):
    if not fam.get("family") or not fam.get("fields"): F(f"event family malformed: {fam}")

# --- Domain packs (ratified 2026-11-01): subordinate 00-INDEXes + catalogue ---
cat_path = ROOT / "docs/contracts/constitution/v1/constitution-catalogue.json"
if cat_path.is_file():
    cat = json.loads(cat_path.read_text())
    for entry in cat.get("packs", []):
        if not (ROOT / entry["path"]).is_file():
            F(f"catalogue rule {entry['rule_id']}: pack file missing: {entry['path']}")
    root_rail_files = set(cat.get("root_rails", {}).values())
    for rf in root_rail_files:
        if not (ROOT / rf).is_file():
            F(f"catalogue root rail missing: {rf}")
else:
    W("constitution catalogue not found (packs optional at this stage)")

for w in warn: print(f"constitution: WARN {w}", file=sys.stderr)
if fail:
    for f_ in fail: print(f"constitution: FAIL {f_}", file=sys.stderr)
    print(f"constitution: {len(fail)} failure(s)", file=sys.stderr)
    sys.exit(1)
print("constitution: OK (structural L1 + zero-deviation + registries)")
