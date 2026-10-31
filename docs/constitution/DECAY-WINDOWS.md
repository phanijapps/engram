# Decay windows — temporal clauses of rules

A decay window is a clause of the rule itself: a measured baseline with a closure
trajectory and defined gate behavior during the window. It is NOT a waiver; the terminal
gate MUST go strict on the closure date. Format: rule ref, baseline, trajectory, gate
behavior during window, closure date.

| # | Rule ref | Baseline (measured 2026-10-31) | Closure trajectory | Gate behavior during window | Closure date |
|---|---|---|---|---|---|
| DW-1 | OR-003 (AGENTS.md tree-shape truth) | CLOSED 2026-10-31: adapters/ block corrected to disk truth; core/packs + core/proposals declared | closed in same-day job | closed | 2026-10-31 |
| DW-2 | DD-003 (no committed build artifacts) | CLOSED 2026-10-31: both launcher tarballs git-rm'd; `*.tgz` ignored | closed in same-day job | closed | 2026-10-31 |
| DW-3 | OR-004 (job-layout policy) | CLOSED 2026-10-31: job-010 archived — 2408 items moved under runs/; job-dir top level = 11 canonical entries | closed | closed | 2026-10-31 |
| DW-4 | PC-002 / hash pinning (SDD artifacts) | 6 of 383 declared input hashes mismatch across historical jobs (prototype gate run) | New jobs pin correctly (template v2); historical mismatches recorded, not rewritten | Advisory (report counts) | 2026-11-28 |
| DW-5 | SDD frontmatter baseline (PC-002 precondition) | 6 briefs lack frontmatter (pre-template-v2 jobs); all analysis/plan/tasks lack machine frontmatter | Template v2 applies to new jobs; historical jobs stay as-is | Advisory until one clean job cycle completes, then `check_job_artifacts.py` flips blocking for post-adoption jobs | 2026-11-28 |
| DW-6 | Docs gate inert half | RETARGETED 2026-10-31 to real workspace roots (core/adapters/backends/bindings/codegraph/mcp); measured baseline: 14 missing + 63 thin module docs (77 files of 414) PLUS rustdoc `-D warnings` 30 errors / 5 crates (engram-code, codegraph-queries, eval, ingest, knowledge — never gated before, the inert guard hid them). Rust half fully advisory until closure; `CHECK_CODE_DOCS_STRICT=1` enforces now | Docs brought to standard; gate flips strict at close | Advisory (77 issues, shrinking) | 2026-11-28 |
| DW-9 | Script-tree python consolidation (last bash harness) | `tools/dev/pack_catalogue_check.sh` (311-ln 5-leg pack-catalogue harness: pinned fixture catalogue, installed==33 assertion, license audit, --offline-repeat) still bash — a faithful port needs leg-by-leg translation, not a reconstruction | Port legs 1-5 + offline-repeat mode to tools/dev/pack_catalogue_check.py with identical assertions | Advisory (dev-only, zero CI references) | 2026-11-28 |
| DW-8 | CE-001 (L3 evidence coverage) | Jobs 001-010 predate L3 wiring (2026-10-31): no events.jsonl; validated only when present | Historical jobs archived/closed; new jobs emit via factory.py emit_evidence | Advisory for jobs <= 010; strict for job 011+ (gate enforces) | 2026-12-15 |
| DW-7 | EN-004 (fail-closed neutrality paths) | `check_engine_neutrality.py` silently skips missing declared paths | Add fail-closed handling for absent paths | Gate stays green only while all declared paths exist (current state) | 2026-12-15 |

## Closure record

| Window | Closed | Note |
|---|---|---|
| DW-7 EN-004 fail-closed | 2026-10-31 | Closed 22 days early by job 011 (v2 pipeline first run: 66% artifact-noise cut, 143 vs ~418 ln). |
| DW-3 job-layout archival | 2026-10-31 | (also closed above) |
| DW-1 AGENTS.md tree-shape truth | 2026-10-31 | adapters/ block corrected to disk truth; core/packs + core/proposals declared. Component registry already matched disk (seeded from workspace members). |
| DW-2 committed build artifacts | 2026-10-31 | 2 launcher tarballs `git rm`'d; `*.tgz` ignored. |
