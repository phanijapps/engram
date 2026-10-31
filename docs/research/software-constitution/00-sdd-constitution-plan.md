# Plan: Governing SDD with a Software Constitution — synthesis and adoption plan

Date: 2026-10-31
Status: proposal (owner ratification required — see §7)
Analysts: three independent subagent perspectives —
[gpt-6.1-sol] constitution architecture (`01-constitution-organization.md`),
[gpt-5.6-terra] SDD template redesign (`02-sdd-templates.md`),
[glm-5.3] gates & policies with live prototype runs against all 10 jobs (`03-gates-policies.md`).
This file synthesizes them and adds the orchestrator's cross-cutting findings.

Index-mining record (mandatory): `recall` for "constitution gates", "factory job rework
noisy template", "CI gate engine neutrality parity", "job artifact verification gate" →
all `hit_count: 0` — **index empty for all constitution/SDD-template queries**. The one
live index finding: `tools/hooks/index-mining.py` prints a `search` subcommand the
`engram-mcp` binary does not implement (stale instruction — fix separately).

## 0. The problem, restated as evidence

1. **Template ceremony**: jobs 001–009 average ~1,000 lines across ~9 artifacts; small jobs
   (006 zstd advisory, 007 clippy cleanup) carried ~800 lines of SDD markdown for cleanup
   work. Job 002's single requirement is restated in 6+ sections across 4 artifacts
   (`02-sdd-templates.md` §Duplication trace).
2. **Orchestration sprawl**: job 010 accumulated **874 files / 23 MB** in its job dir —
   dispatch results, batch receipts, recovery/rerun churn at top level, drowning the 9
   canonical artifacts (orchestrator measurement + `03-gates-policies.md` §5).
3. **Late detection**: nothing checks an artifact until human review — job 002 shipped a
   live `AC-013` coverage violation; 6/383 declared input hashes mismatch; 6 briefs lack
   frontmatter (prototype gate run over all 76 real artifacts, `03-gates-policies.md` §2a).
4. **Governance prose is unenforced**: AGENTS.md's excellent boundary rules (engine
   neutrality, surface parity) have gates; most other rules (no god modules — partially,
   tree shape, distribution drift) have none or inert ones (`check-code-docs.sh` gates a
   `crates/` dir that doesn't exist in this layout).

The Software Constitution standard's answer to exactly this: "wiki pages don't enforce
design principles — gates do." Adopt its *structure*, map engram's rules into it, and
gate the SDD pipeline itself.

## 1. Q1 — How to organize the Constitution (engram-specific)

Full detail in `01-constitution-organization.md` §1–4. Shape:

```
constitution/                      # human-readable rails (the law)
  00-INDEX.md                      # master index, universal rules, rail↔gate table
  01-engine-neutrality.md          # RULE-1, RULE-6
  02-surface-parity.md             # RULE-1, RULE-6
  03-canonical-implementations.md  # RULE-1
  04-distribution-drift.md         # RULE-2 (published-surface reading)
  05-production-completeness.md    # RULE-3
  06-canonical-evidence.md         # RULE-4
  07-zero-deviations.md            # RULE-5
  08-coherent-integration.md       # RULE-6
  DEVIATIONS.md                    # empty Active section on day one
  DECAY-WINDOWS.md                 # see §6 below for day-one windows
contracts/constitution/v1/         # machine truth (spec-root; fits existing contracts/)
  implementation/registry.json     # capability → canonical owner (extends ADR-0022 grid)
  events/registry.json             # evidence event families (declared before referenced)
  gates/registry.json              # rule → gate mapping
  distributions/registry.json      # npm/crates/launcher surfaces (RULE-2)
  components/registry.json         # crate/package/transport inventory (RULE-6)
  schemas/*.json                   # JSON Schema 2020-12 for all of the above + SDD artifacts
scripts/ci/                        # gates-root (extends the existing set)
  + check-constitution.sh          # index/registry schema + zero-deviation umbrella
  + check-job-artifacts.sh         # SDD pipeline gates (§5 below)
  + check-implementation-registry.py, check-coherent-integration.py,
    check-production-completeness.py, check-distribution-drift.py, check-canonical-evidence.py
```

Key decisions embedded there: rails mirror the boundary rules already proven in
AGENTS.md/ADRs (engine neutrality, surface parity) so the constitution *ratifies existing
law* rather than inventing new law; RULE-2 is honestly reinterpreted for a library/SDK
repo as **source ↔ published surface ↔ installed agent bundle reconciliation** (npm file
lists, crates.io, `packages/launcher/skills+agents` vs `.agents/skills` — a real drift
risk today: 2 stale tarballs sit tracked in the launcher).

Division of labor: AGENTS.md = how to *work* here (guidance, agent-facing); constitution/
= what is *enforced* (gate-backed, merge-blocking); docs/adr/ = why a decision was made;
docs/specs/ = feature acceptance criteria (now carrying constitution rule IDs).

## 2. Q2 — Brief (intent framing), enhanced

Keep: MoSCoW, decision ledger, owner ratification, non-goals (all strengths).
Add (`02-sdd-templates.md` Template 1):

- **Frontmatter** (machine-checkable): `schema_version, artifact: brief, job_id, status:
  draft|ratified, size_tier: S|M|L, owner, ratified_at, constitution_rules_touched: []`.
- **Problem, not symptom** section (separate from the ask).
- **Tier field** that *scales downstream ceremony* (the direct noise fix — §6).
- Ledger IDs (`LED-*`) carried forward by reference, never restated.

## 3. Q3 — Spec (rename `analysis.md` → `spec.md`)

`02-sdd-templates.md` Template 2. The restructure that matters:

- **Intent trace**: `brief_ref {path, sha256}` + MOS→REQ disposition table — kills the
  6× restatement pattern by making downstream artifacts *reference* IDs, not copy prose.
- **Requirements as EARS-style rows**: `REQ-ID | MUST/SHOULD | When <trigger>, the system
  shall <observable> | rule IDs | evidence IDs`.
- **Acceptance bound to named gates**: `AC-ID | REQ | Given/When/Then | GATE-ID |
  negative case` — the field that lets CI catch the job-002 AC-013 class of violation.
- **Evidence as `path:line` citations** (gate verifies paths exist); scope boundaries as
  an explicit out-list; terms/environment sections only when tier requires.

## 4. Q4 — Plan

Every step is a table row: `STEP-ID | imperative action | REQ IDs | constitution rule IDs
| concrete paths | verify (GATE-ID or command) | rollback`. Two invariants a gate can
check: **no orphan steps** (every step ≥1 REQ) and **no orphan REQs** (every REQ ≥1
step). Blast radius and contracts sections retained but required only when public
surfaces change.

## 5. Q5 — Tasks

Checkbox-first table: `[ ] | TASK-ID | outcome | STEP IDs | REQ IDs | gate to run |
evidence ref`, plus a **REQ×task coverage matrix** (CI-verifiable totality — the AC-013
fix generalized) and a gate inventory. Rework tracking becomes *links to review findings*
(post-review only), not a restated section.

## 6. Q6 — Gates, policies, tiering, decay windows

From `03-gates-policies.md` (prototyped against all 10 jobs — real numbers):

**Tiering policy (S/M/L)** — required/optional/omitted per section per artifact
(`02-sdd-templates.md` §Tiering). Measured against the two real small jobs (2026-10-31,
section-heading audit of analysis/plan/tasks): job 007 423→~226 ln, job 006 419→~244 ln —
**~44% artifact-volume cut for S-jobs**; L-jobs stay similar length by design (value =
gate-ability, not brevity). Commitment: re-measure after the first tiered S-job; if the cut
is <30%, re-tune the tiering table rather than defend it.

**New gates**: `check-job-artifacts.sh` (frontmatter schema · REQ↔STEP↔TASK referential
integrity · coverage totality · no TBD/TODO in ratified sections · input-hash pinning ·
ratification-before-build) + rule-class gates: orphan-crate (RULE-6, registry-backed),
duplicate-capability (RULE-1), stub-marker scoped to shipped code (RULE-3),
evidence-event for approve/rework/release (RULE-4), distribution drift (RULE-2).

**Policies** (owner/board-level): ratification (what merges without owner sign-off —
nothing touching rails/registries/contracts), deviation approval (owner, time-boxed, max
active count), tiering (which gates run per S/M/L), amendment (constitution changes
update gate mapping + evidence in the same review).

**Day-one decay windows** (real debt, time-boxed not waived): TODO/FIXME baseline in
tracked first-party code; AGENTS.md tree-shape drift (declares paths that moved);
2 stale launcher tarballs; job-010 874-file sprawl; 6/383 input-hash mismatches;
inert `check-code-docs.sh` Rust half.

**Sequencing**: Week 1 — templates v2 + `check-job-artifacts.sh` (advisory→blocking) +
constitution scaffolding = **L1**. Month 1 — registries + orphan/duplicate/distribution
gates blocking = **L2**. Month 2–3 — evidence events on every factory privileged action
= **L3**. **Skip L4** (platform-bijected meta-index is KYE-shaped, not library-shaped).

**SCCT honesty**: the reference checker matches gate *filenames* for L2 and marker
strings for L3 — naming our gates to match patterns earns the badge cheaply; the value is
the internal discipline. Also: the standard itself drifts (site v1.1 says six rule
classes, org README five, SPEC.md v1.0 six) — pin to SPEC.md §4 and record the pin in
00-INDEX.md.

**Orchestrator addition (job-layout policy)**: fix orchestration sprawl at the skill
level — `engram-factory` SKILL.md gets a rule that the job-dir top level holds only the
9 canonical artifacts; receipts/dispatch results/rerun/recovery churn go under
`runs/<batch-id>/`. A layout gate (`ls` allowlist) enforces it. No template change can
fix job 010's 874 files; this can.

## 7. Ratification asks (owner decides)

1. Adopt the constitution topology in §1 (rails 01–08, spec-root `contracts/constitution/v1`,
   gates-root `scripts/ci/`).
2. Approve template v2 set (brief/spec/plan/tasks with frontmatter + ID schemes) and the
   S/M/L tiering table.
3. Approve rename `analysis.md` → `spec.md` (breaking for in-flight jobs; grandfather).
4. Approve job-layout policy (§6 last para) + gate.
5. Approve week-1/month-1/later sequencing and day-one decay windows.
6. Decide: SCCT pin to SPEC.md §4 (recommended) and skip-L4 stance.

## 8. Execution order once ratified

**EXECUTION RECORD (2026-10-31, same day as ratification) — phase 1 + phase 2
template core landed:**

- `constitution/` live: 00-INDEX + 8 rails + DEVIATIONS (Active empty) +
  DECAY-WINDOWS (7 day-one windows with measured baselines).
- `contracts/constitution/v1/` live: implementation/gates/events/distributions/
  components registries seeded with real inventory (35 crates + 6 packages) +
  vendored canonical registry schema + SDD frontmatter schema.
- Gates: `scripts/ci/check-constitution.sh` (blocking; mutation-tested — active
  deviation and dangling registry path both fail) and
  `scripts/ci/check-job-artifacts.sh` (advisory; 79 findings on 10 historical
  jobs, correctly exit 0; flips blocking 2026-11-28 per DW-5). Both wired into
  ci.yml (artifact gate continue-on-error; it bites locally where
  `.engram-factory/` lives).
- Templates v2 live in factory assets (brief/analysis/spec-genre/plan/tasks)
  with frontmatter + REQ/STEP/TASK/AC/GATE ID schemes; engram-factory SKILL.md
  gained the Constitution compliance section, the S/M/L tiering table, and the
  runs/ job-layout policy; AGENTS.md Validation updated.
- SCCT reference checker verdict: **L1** (structural). L2 pending planned
  rule-class gates.
- Deferred follow-ups (as planned): filename rename analysis.md→spec.md
  (touches factory.py state machine + 4 agent briefs — its own small job);
  month-1 rule-class gates (check-implementation-registry.py, orphan gate,
  distribution drift, production-completeness marker gate); L3 evidence-event
  wiring; DW-1/DW-2 cleanup PRs (AGENTS.md tree truth, stale tarballs).

Original sequencing (superseded by the record above where advanced):

1. Factory job S: land `constitution/` scaffolding + `contracts/constitution/v1/schemas`
   + `check-constitution.sh` (L1).
2. Factory job M: templates v2 into `.agents/skills/engram-factory/assets/templates/` +
   SKILL.md tiering/job-layout rules + `check-job-artifacts.sh` advisory on all repos.
3. Flip advisory→blocking after one clean job cycle; open decay windows; registries +
   rule-class gates (L2); evidence events last (L3).

Artifacts: `01-constitution-organization.md` (292 ln), `02-sdd-templates.md` (319 ln),
`03-gates-policies.md` (210 ln) — each carries its own evidence record and validator pass.

## 9. M-tier job record (2026-10-31, same day) — docs centralization + rename + DW closures

Owner chose Option A (front door + symlinks, SCCT L1 preserved):

- `docs/README.md` — the front-door master index (constitution, contracts,
  architecture, domain truth, ADRs/specs/RFCs/research, guides).
- Symlinks: `docs/constitution -> ../constitution`, `docs/contracts -> ../contracts`.
- Leaked HTML artifacts relocated to `docs/assets/artifacts/` (knowledge-graph.html
  1.4MB, engram-deck.html, cleanup-plan.html — all unreferenced).
- `architecture.md` kept: it is a CI-pinned pointer (check-contracts.sh asserts it),
  now indexed from the front door rather than duplicated.
- **Rename executed**: `analysis.md` -> `spec.md` across factory.py (6 sites),
  templates, SKILL.md, 4 analyzer agent dialects, references (`## Analysis` -> `## Spec`
  headings), evals — 48/48 factory tests pass.
- DW-1 closed: AGENTS.md adapters/ tree corrected to disk truth; core/packs +
  core/proposals declared.
- DW-2 closed: 2 launcher tarballs `git rm`'d; `*.tgz` ignored.
- Gates: check-constitution.sh strengthened (closure-record scoping) and green;
  full battery green (docs, contracts, neutrality, parity); SCCT: L1.

## 10. Owner-directed relocation (2026-10-31, later same day)

Owner chose literal centralization over symlinks: `constitution/` -> `docs/constitution/`,
`contracts/` -> `docs/contracts/` (40+ external reference sites rewritten: generation
tooling, CI manifests, AGENTS/CONTRIBUTING/README/GOVERNANCE, registries, rails).
Recorded as a deliberate deviation from SPEC.md §3 root paths in the 00-INDEX
amendment record; SCCT therefore reports L0 by layout choice while internal gates
enforce the same rules. Also retired: ADRs 0002 (status-less), 0014 + 0023-era stale
Proposed 0014, Superseded 0018 (0002, 0014, 0018 unreferenced; 0023/0024 kept — each
has 1 external ref); stray perf/context/superpowers dirs and remaining HTMLs removed
earlier. gates + contracts:generate + docs/neutrality gates green.

## 11. L2 gate set landed (2026-10-31, "next jobs" round)

Four rule-class gates implemented, mutation-tested, registry-flipped to
existing+blocking, wired into ci.yml: check-implementation-registry.py (RULE-1
duplicates + wrapper-imports-SSOT), check-coherent-integration.py (RULE-6
bidirectional 44 components), check-production-completeness.py (RULE-3 markers
+ PR-002 LLM-SDK dependency lint), check-distribution-drift.py (RULE-2
launcher-payload sync + no tracked artifacts + npm files declared). All green;
each proven to fail on a planted violation (orphan, TODO marker, skill desync;
tgz unplantable — DW-2 ignore rule blocks it). Rail 09 (Principles) added
earlier this round. DW-6 resolved as retarget: check-code-docs.sh now scans the
real workspace (was inert on nonexistent crates/), advisory with measured
baseline 77 (14 missing + 63 thin), strict 2026-11-28. Remaining: L3 evidence
wiring (month 2-3), DW-3/4/5/7, advisory→blocking flip 2026-11-28.

## 12. L3 evidence wiring landed (2026-10-31)

factory.py emits canonical evidence (rail 06) at the four privileged
transitions — create (job.created), approve (job.approved with artifact
hashes), rework (job.reworked), merge (job.released with merge commit) — to
<job>/events.jsonl using the registry-declared families (declared canonical-
first at adoption). New gate check-canonical-evidence.py (CE-001/CE-002):
fails on undeclared families, malformed records, missing per-family fields,
and post-adoption jobs reaching approve/merge without evidence; mutation-
tested (undeclared `job.teleported` caught). 48/48 factory evals green with
emission active. DW-8 opens the pre-L3 window (jobs 001-010). Registry
flipped to existing+blocking; wired into ci.yml. Conformance: L2 + L3
wiring at docs/ paths (SCCT L0 by recorded layout deviation).

## 13. Script-tree consolidation (2026-10-31, all-Python)

scripts/ is now all-Python, snake_case, stdlib-only, cross-platform: shared
lib (scripts/lib/repo.py), gates in scripts/ci (8 shells ported — constitution,
job-artifacts via faithful heredoc extraction; contracts, docs, code-docs,
engine-neutrality, surface-parity hand-ported; research-parity wrapper
eliminated), impl checkers moved in from tools/scripts (check_docs.py IMPORTS
them — wrapper/impl split gone), codegen to scripts/contracts/, clean-workspace
ported from mjs. 39 reference files updated (ci.yml, AGENTS.md, registries,
constitution rails, guides, package.jsons, rust doc comments). Components
registry now declares the scripts tree + tools/hooks (RULE-6 gap closed).
Owner pruned tools/hooks to the index-mining trio. pack_catalogue_check.sh
restored as-is after a reconstruction attempt failed fidelity review — DW-9
opened for a faithful port. tools/ now holds only hooks/.

## 14. Unified under tools/ (2026-10-31, owner direction)

scripts/ merged into tools/: ci/ contracts/ dev/ release/ lib/ now live beside
hooks/ — one tree for all repository operations, all-Python. 30+ live
reference files swept (ci.yml, AGENTS.md, gates+components registries,
package.jsons, launcher tests, constitution rails, guides, examples); gates
registry paths and the constitution's gates-root declaration now read tools/ci.
Two untracked casualties of the move (lib/repo.py, scripts/README.md) were
recreated from known content — lesson: stage before git-mv of untracked dirs.
Battery: 12/12 gates green, 48/48 factory evals, AGENTS.md tree updated with
the tools/ layout. tools/README.md documents conventions.

## 15. Template contract reconciliation + downstream v2 (2026-10-31)

The v2 templates are now CONTROLLER-VALIDATABLE: union frontmatter (v2 keys
artifact/size_tier/constitution_rules_touched/brief_ref + v1 load-bearing keys
with render placeholders), JSON-per-line values (metadata() is a JSON-per-line
parser — the original v2 YAML-flow frontmatter would have crashed it),
frontmatter-first files (controller requires startswith ---), factory.py
accepts schema_version 2 + `artifact` as identity alias with either/or check.
Proven end-to-end: render("spec") → fill → validate_artifact PASS (identity
spec, schema 2, tier S, hash-pinned inputs). Also: stale tool paths fixed in
all four v2 template headers; verification.md upgraded to the GATE-evidence
ledger (AC×GATE rows, NOT_RUN is a finding); release.md gained the L3
evidence-chain section; engram-frame Contract §0 makes briefs v2 at the
source (tier question last). 48/48 evals green (v1 fixtures still pass —
union accepts both). Next: first real v2 job (DW-3 archival candidate).
