# Gates & Policies — mapping the Software Constitution onto engram (Q6)

Date: 2026-10-31 · Perspective: CI enforcement / devops
Companion to `_briefs/03-gates-policies.md` (owner question Q6: appropriate gates and
policies applicable via constitution).

## Sources verified (fetched 2026-10-31)

- <https://softwareconstitution.com/#rules> — site v1.1: six rule classes, **five**
  levels L0–L4 (v1.1 adds `L0 None`), conformance bar "≥1 passing gate per class, on
  every push", `npx scct <repo>`, reference gates ship in Node.js + Python.
- <https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md> — v1.0
  (Draft/RFC, adopted 2026-05-19): required artefacts (`constitution/00-INDEX.md`,
  numbered rail docs, `DEVIATIONS.md`, `DECAY-WINDOWS.md`, `implementation/registry.json`,
  `schemas/`, `gates/`), rule classes §RULE-1..6, levels L1–L4, registry schema
  (`concept_id` / `ssot_module` / `transport_wrappers[].must_import`), decay-window format
  (baseline → trajectory → gate behavior), semver amendment process.
- Level-set discrepancy noted: site v1.1 says L0–L4; SPEC.md v1.0 says L1–L4. We cite both.

## Index-mining results (mandatory pre-work; recorded honestly)

All four recall queries returned `hit_count: 0` (matching the parent's observation that
template-adjacent queries are empty):

| Query (`engram-mcp recall --json --limit 5 --storage ~/.engram --project agentzero`) | Result |
|---|---|
| `"CI gate engine neutrality parity"` | `hit_count: 0`, `top_score: 0.0` |
| `"job artifact verification gate"` | `hit_count: 0` |
| `"engram-factory job cycle rework review evidence"` | `index empty for query` |
| `"constitution deviations registry gates"` | `index empty for query` |

Code index: `engram-mcp code --op status` fails with `BindingRequired` (no repo bound to
this project in the one-shot CLI); the CLI build has no `search` subcommand. **Index empty
for all gates/constitution queries** — this mapping is therefore built from primary
sources read directly (below), not from memory.

## Files read (evidence base)

`scripts/ci/`: check-contracts.sh, check-engine-neutrality.sh, check-surface-parity.sh,
check-docs.sh, check-code-docs.sh, check-research-parity-docs.sh, check-bound-code-matrix.py,
prepare-acquisition-fixtures.py; `tools/scripts/`: validate_contracts.py,
check_architecture_guards.py, check_ts_native_delegation.py, check_research_parity_docs.py;
`.github/workflows/ci.yml`, `release.yml`, `release-verify.yml`, `portable-acquisition.yml`;
`.agents/skills/engram-factory/SKILL.md` + `assets/templates/*.md` (8 templates) +
`assets/board.schema.json`; `.engram-factory/board.json`, `jobs/` (10 jobs; 002/006/007
read in depth; 010 inventoried); `AGENTS.md`; root `Cargo.toml`; `pnpm-workspace.yaml`;
`packages/launcher/package.json` + tree; `.gitignore`, `.git/info/exclude`.

---

## 1. Inventory & mapping: existing gates → rule classes

Honesty note: most existing gates are **rule-class-adjacent**, not exact-class matches.
The exact-class fit is mostly still to be built (§2). "Runs in CI" = wired into
`.github/workflows/ci.yml`.

| Existing gate | What it checks | Closest rule class | Gap notes |
|---|---|---|---|
| `check-contracts.sh` (CI: contract job) | required-file manifest for `contracts/v1` + `docs/architecture/domain-data-model.md` sections; jq JSON syntax; `training_export` exclusion; JSON-Schema 2020-12 validation via `validate_contracts.py` incl. invalid examples | RULE-6 (declared↔disk), RULE-1-adjacent (single contract SSOT) | One-directional: required files must exist, but nothing stops *undeclared* new contract files (orphan direction missing) |
| `check-engine-neutrality.sh` (CI: rust job) | deny-list grep over 20 gated paths: engine type names (`Sql*`,`Pg*`,`Surreal*`,`Lance*`,`Tantivy*`), engine-crate imports, raw-SQL literals | RULE-1 (single implementation; neutral ports may not fork per engine) | Header documents **intentionally ungated deferred-debt layers** (engram-runtime, core/eval, config.rs, bindings/node) — an undocumented decay-window list living in a comment |
| `check-surface-parity.sh` (CI: rust job) | every facade `require_*` has an N-API `require*Api` proxy; `SPECIAL_CASES` + `DEBT_ALLOWLIST` escape hatches | RULE-1 exact fit (registry's "transport_wrappers must import SSOT") + RULE-5-adjacent (`DEBT_ALLOWLIST` is an active-deviations list; currently empty ✓) | Facade→binding direction only; no binding-only surface check |
| `tools/scripts/check_ts_native_delegation.py` (via check-docs.sh, CI: docs job) | TS must call native for 8 required calls; 6 named reimplementations forbidden (`cosineSimilarity`, …) | RULE-1 exact fit (wrapper may not reimplement SSOT) | Hardcoded name lists in the script = a hand-rolled mini-registry |
| `tools/scripts/check_architecture_guards.py` (CI via check-docs.sh) | crate-root `lib.rs` ≤260 lines, behavior markers banned in roots; `packages/*/src/index.ts` ≤120 lines, facade-only | RULE-1-adjacent (anti-god-module) | Line thresholds are policy constants, not declared rules |
| `check-docs.sh` (CI: docs job) | TODO markers in tracked docs/contracts/skills (`*.upstream.md` exempt), skill validator, then chains the three scripts above + `check-research-parity-docs.sh` | RULE-3 for docs | Marker pattern is only `TODO` / `\[TODO` — no `FIXME`/`TBD`/`XXX` |
| `check-code-docs.sh` (CI via check-docs.sh) | module docs ≥120 chars, pub item docs ≥80 chars; `cargo doc -D warnings` | RULE-3-adjacent (docs quality) | **Rust half is inert**: it gates `[[ -d crates ]]` but this repo has no `crates/` dir (layout is `core/`, `adapters/`, …). Only the TS half runs. Orphan-gate finding |
| `check-research-parity-docs.sh` (CI via check-docs.sh) | explicit drift registry (`tools/research-parity/doc-drift-registry.json`) for research-doc claims vs implementation | RULE-2-adjacent (declared claims vs actual state) | Small curated registry; good precedent for DECLARED drift tracking |
| `check-bound-code-matrix.py` (not in CI; job-010 local/edge gate) | fail-closed operation×backend×transport matrix, per-row persisted receipts, secret redaction | RULE-4-adjacent (evidence events per run) | Exactly the L3 evidence pattern, but local-only and job-specific |
| ci.yml typescript job step `git diff --exit-code -- packages/contracts/src/generated` after `pnpm run contracts:generate` | generated contracts must equal committed ones | RULE-2 in repo-internal form (declared == generated) | Could generalize to launcher payload mirror (§2c) |
| `prepare-acquisition-fixtures.py` (CI) | fixture prep for ingest tests | none (support) | — |
| AGENTS.md "Validation" list (`cargo fmt/check`, `pnpm contracts:generate/typecheck/test/build`) | fmt/clippy/tests/TS build wired in ci.yml; `cargo clippy --all-targets` subsumes `cargo check` | hygiene baseline | `check-code-docs.sh`'s `cargo doc` and `check-bound-code-matrix.py` are local-only by design (slow / needs live services) |

## 2. New gates proposed

**Where SDD gates run (key architectural fact):** `.engram-factory/` is fully
git-ignored — not via `.gitignore` but via machine-local `.git/info/exclude`
(`/.engram-factory/`). GitHub CI can never see job artifacts, so **factory-artifact gates
run in the factory loop** (invoked by `.agents/skills/engram-factory/scripts/factory.py`
at each stage advance / Build Check), while **repo gates run in ci.yml**. Secondary
finding: the ignore rule itself is undeclared in-repo — a fresh clone sees `.engram-factory/`
as untracked noise; moving it to `.gitignore` is a one-line hygiene fix.

### 2a. SDD-artifact gates (the owner's "catch bad artifacts before expensive review")

Real yield measured today (10 jobs, 76 core artifacts audited by prototype script):

- 6/76 artifacts lack parseable frontmatter — **all six are `brief.md`** (owner-authored;
  the brief template ships no frontmatter). All factory-authored types conform.
- 383 `input_artifacts` sha256 hashes declared across jobs; **6 mismatch or dangle**
  (edited-after-registration or missing file) — 1.6%.
- REQ↔AC↔TASK integrity: jobs 006/007 clean; **job 002 declares `AC-013` in
  `analysis.md` but `tasks.md` neither covers it nor names it in a reasoned omission** —
  a live referential-integrity violation the gate would catch today.

| Gate (name) | Contract ("fails when…") | Sketch (≤10 lines) | Cost | Mode |
|---|---|---|---|---|
| `check-job-artifacts.sh` — frontmatter | fails when any factory artifact (analysis/plan/tasks/build/verification/release/decision) misses required frontmatter keys (`schema_version, artifact_type, job_id, cycle, status, author_role, input_artifacts, updated_at`), or `artifact_type` ≠ filename, or `job_id` ≠ dir name | `for f in brief analysis plan tasks build verification release decision; do` `awk '/^---$/{n++;next} n==1' $f.md \| grep -q "artifact_type:" \|\| fail; done` + per-type required-key map | fast (<1 s/job) | blocking, in Build Check + stage advance |
| `check-job-artifacts.sh` — ID referential integrity | fails when a `TASK-###` "Covers" cell references an `AC-###` absent from `analysis.md` (dangling), or an `AC-###` is covered by no task and appears in no "reasoned omission" (totality), or an `AC-###`'s `REQ-###` column references an undeclared `REQ-###` | `acs=$(grep -oE 'AC-[0-9]{3}' analysis.md \| sort -u)`; `cov=$(awk -F'\|' '/^\| TASK-/{print $5}' tasks.md \| grep -oE 'AC-[0-9]{3}')`; `comm -23 <(echo "$acs") <(echo $cov)` → must appear under `## Coverage` omission lines; same for `REQ-` back-refs | fast | blocking |
| `check-job-artifacts.sh` — input-hash pinning | fails when a declared `input_artifacts[].sha256` ≠ sha256 of the file on disk (artifacts edited after registration) | `python3 -c 'for i in fm["input_artifacts"]: assert sha256(job/i["path"]) == i["sha256"]'` | fast | blocking (already caught 6 real cases) |
| `check-job-artifacts.sh` — RULE-3 for specs | fails when a ratified-section table cell contains `TBD`, `TODO`, `FIXME`, `<placeholder>` in analysis/plan/tasks (extends `check-docs.sh`'s marker idea to SDD artifacts) | `grep -nE '\b(TBD\|TODO\|FIXME)\b\|<placeholder>' analysis.md plan.md tasks.md` (frontmatter + quoted-owner-text lines exempt) | fast | blocking |
| `check-job-artifacts.sh` — ratification-before-build (RULE-4 applied to job lifecycle) | fails when `board.json.stage == "build"` but brief `Status` row lacks an owner-ratification marker (`ratified <date> by <owner>` or `supersedes:` chain), or when a `decision.md` recommendation is APPROVE without a matching board `approval` snapshot | `jq -r '.jobs[] \| select(.stage=="build")' .engram-factory/board.json`; check `brief.md` table row `Status` matches `drafted/ratified` grammar | fast | blocking |

Rationale: all anchors already exist — templates carry uniform frontmatter
(`.agents/skills/engram-factory/assets/templates/*.md`), `tasks.md` v2 has a
`Covers` column and a `## Coverage` omission section, `analysis.md` has `## Requirements`
(`REQ-###`) and `## Acceptance` (`AC-###`), and `board.json` carries stage/cycle/round/
approval. The gate is pure parsing over existing structure — no pipeline change.

### 2b. Rule-class gates for a library/SDK repo

| Gate | Contract | Sketch | Cost | Mode |
|---|---|---|---|---|
| **Orphan-crate gate** (RULE-6) — fails when a dir with `Cargo.toml`/`package.json` under `core/ adapters/ bindings/ backends/ mcp/ codegraph/ packages/ examples/ prototype/` is on disk but not declared, or declared but missing | registry = `contracts/registry.json` (schema per SPEC §6, one entry per component): `{"components":[{"id":"memory.store","kind":"crate","path":"adapters/sqlite","package":"engram-store-sqlite","class":"adapter","excluded":[]}]}`; seed data measured: **35 workspace members + 3 intentional non-members** (`adapters/ingest/tests/windows-job-compile` fixture, `core/lang/grammars/tree-sitter-cobol`, `.../tree-sitter-sql` vendored grammars — these are declared-as-excluded, not orphans); pnpm: 7 `packages/*` + 4 `prototype/*` package.json | `find $ROOTS -name Cargo.toml \| comm - <(jq -r '.components[].path' registry.json)` both directions | fast | blocking after seed (week 1 seed is mechanical) |
| **Duplicate-capability gate** (RULE-1) — fails when two registry entries claim the same `id` prefix/capability class, or when a scan finds a second implementation of a declared SSOT (crate exposing the same primary trait or the same npm capability name) | `jq -r '.components[].id' registry.json \| sort \| uniq -d` → fail; plus class-level pairs (e.g. two `retrieval.index` engines must differ in `engine` field and both be declared) | fast | blocking |
| **Stub-marker gate, scoped to shipped code** (RULE-3) — fails on `TODO\|FIXME\|XXX\|todo!\|unimplemented!\|unreachable!("stub")` in non-test, non-vendored shipped code | `git ls-files 'core/**' 'adapters/**' 'bindings/**' 'mcp/**' 'backends/**' 'packages/*/src/**' \| grep -vE '(tests?\|examples)/' \| xargs grep -nE pattern` with explicit exemptions: `vendor/`, `*.upstream.md`, `#[cfg(test)]` blocks (python filter), documented `unreachable!` with justification comment | fast | **blocking day one — debt is near zero** (real numbers in §5) |
| **Evidence-event gate** (RULE-4) — fails when a factory privileged action (approve / rework / release / merge) has no canonical evidence event, or an artifact references an event family the registry hasn't declared | the factory **already emits de facto evidence**: sha256 `input_artifacts`, `source-*.json` snapshots, per-command `*.provenance.json` logs, per-round `gate-check.py` (all observed in jobs 002/010). Canonicalize as `events.jsonl` per job: `{"ts","actor_role","event":"owner.approve","job_id","snapshot_sha"}`; gate = every board transition in `board.json` history has a matching event, and every `event` string ∈ registry-declared families | fast (parse); medium (instrument factory.py) | advisory week 1 → blocking month 2 |

### 2c. RULE-2 honest reinterpretation for a library + MCP-server repo

engram has no single "production"; its deployed surfaces are **published artifacts**.
Proposed interpretation — "prod = what a consumer installs", three bijections:

1. **npm published surface**: `packages/launcher/package.json` `files` list vs `npm pack`
   file list vs repo tree (release.yml already packs + verifies idempotency with
   `npm view`). Gate: `npm pack --dry-run --json` file set ⊆ declared `files` + dist.
   Found debt: **2 stale tarballs are git-tracked** (`packages/launcher/engram-mcp-0.1.0.tgz`,
   `engramlabs-mcp-0.1.7-alpha.tgz` vs current version 0.6.0) — gate would fail today → decay window.
2. **launcher payload mirror**: `packages/launcher/skills|agents/` must byte-equal
   `.agents/skills|agents/` (diff -rq run today: **clean except `__pycache__`** — enforceable day one,
   `git ls-files` both trees + `diff -r`, exempt `__pycache__`). This is the cheapest true
   RULE-2 gate in the repo.
3. **crates.io surface**: no `cargo publish` exists anywhere in `.github/` (verified: 0 hits)
   and 5 workspace packages sit publishable-but-unpublished at 0.1.0. Honest call: **defer**
   with a DECAY-WINDOWS entry ("crates.io publication not a declared surface until first
   publish; gate = workspace member set vs crates.io versions on publish day"). crates.io API
   was unreachable from this environment (fetch-failed) — publication status not verifiable here;
   the absence of any publish workflow is the repo-side fact.

---

## 3. Policy set (owner-level; enforced by humans + board, not scripts)

1. **Ratification policy** — what needs owner approval before merge: (a) any change to
   `constitution/` rules or registry.json (amendment, §3.4); (b) any contract-surface or
   generated-type change (`check-contracts.sh` domain); (c) every factory `decision.md`
   APPROVE (already required: SKILL.md "The approver recommends approval. The human
   authorizes the merge"); (d) any new deviation entry (§3.2); (e) engine/backend
   additions (ADR-0022 territory). Everything else merges on green gates.
2. **Deviation-approval policy** — who: the owner only (single-maintainer repo today);
   time-box: max 30 days per entry with a closure date; max active: 3 (the repo's existing
   `DEBT_ALLOWLIST`/deferred-debt layers show the pattern — engine-neutrality's ungated
   layers are the current de facto deviations). Active section empty is the steady state
   (RULE-5); expiring entries auto-fail CI (gate checks dates in DEVIATIONS.md).
3. **Tiering policy (S/M/L → which gates run)** — *all* pushes: fmt/clippy/test +
   fast static gates (engine-neutrality, surface-parity, stub-marker, contracts, docs) —
   these are all grep/parse class, seconds each. *S jobs* (docs/spec-only diff): + SDD
   artifact gates, no full build. *M jobs* (single-crate code diff): + `cargo test -p`
   for touched crates + generated-contract drift. *L jobs* (cross-crate / bindings /
   contracts / release): full battery incl. `pnpm build`, N-API conformance, bound-code
   matrix, launcher-mirror gate. pgvector conformance already tiers correctly in ci.yml
   (path-filtered live-Postgres job) — that is the precedent to follow.
4. **Amendment policy for the constitution itself** — changes to rule docs are
   numbered-doc edits with an entry in `00-INDEX.md` changelog + owner ratification;
   registry.json changes require a matched `ssot_module` existence check (the orphan gate
   runs both ways, so a bad amendment fails CI). Semver per SPEC §10: breaking schema
   changes bump major and re-validate.

## 4. Sequencing plan

| Phase | Ship | SCCT level reached |
|---|---|---|
| **Week 1** (cheap, high signal) | `check-job-artifacts.sh` (frontmatter + ID integrity + hash pinning + spec-stub markers) wired into factory Build Check; stub-marker gate scoped to shipped code (debt ≈ 0); launcher-mirror gate in ci.yml; fix `.gitignore` for `.engram-factory/`; author `constitution/00-INDEX.md`, DEVIATIONS.md, DECAY-WINDOWS.md + `contracts/registry.json` seed (35 crates + 3 declared-excluded) | **L1 Structural** (artifacts exist, schemas validate, ≥1 rule per class declared) |
| **Weeks 2–4 (month 1)** | orphan-crate + duplicate-capability gates blocking in CI (both directions); deviation-expiry gate (RULE-5); ratification-before-build gate; tiering policy wired as path filters | **L2 Gate-enforced** (every declared rule has a passing, blocking gate on every push) |
| **Later (month 2–3)** | evidence-event family (`events.jsonl` + registry-declared families) emitted on every factory privileged action and every CI run; canonical-first gate (undeclared event refs fail); npm-pack surface gate; revisit crates.io window | **L3 Self-governed** (audit chain per CI run; the repo can already do this — per-command `provenance.json` receipts exist in job-010, they need standardizing, not inventing) |
| **Not targeted** | L4 platform-bijected meta-index — overkill for a library/SDK repo; single deployable surface (launcher) covered by the mirror gate | L3 steady state |

## 5. Decay windows to open day one (real numbers, greps actually run)

RULE-3 code debt is **near zero** — no time-box needed for stubs; the real debt is
drift and noise:

| Debt (measured 2026-10-31) | Real number | Proposed window |
|---|---|---|
| TODO/FIXME in tracked first-party production areas (`core/ adapters/ bindings/ backends/ packages/ mcp/ codegraph/ contracts/ examples/ tools/`) | **0 occurrences** (whole-repo hits: 2 prose quotes in `.claude/.codex` agent briefs, 1 exempt `*.upstream.md`, 1 comment inside check-docs.sh itself, 3 in vendored `vendor/tantivy-sstable`) | none needed — gate strict day one |
| `todo!(` / `unimplemented!(` macros in shipped Rust | **0** | none |
| `unreachable!` in shipped dirs | 10 total; 7 are documented match-arms (e.g. `mcp/engram-mcp/src/config.rs:173` "known flags are matched above"); 3 are `unreachable!("stub")` test fixtures inside `#[cfg(test)]` (`core/knowledge/src/batch_ledger.rs:139,146,153`) | policy: `unreachable!` requires justification comment; test-block stubs exempt |
| **AGENTS.md tree-shape drift** | AGENTS.md declares `adapters/memory/sqlite/`, `adapters/knowledge/sqlite/`, `adapters/hierarchy/sqlite/`, `adapters/retrieval/sqlite-vec/`, `adapters/orchestration/belief-sqlite/` — **none exist**; actual tree is `adapters/sqlite` (one crate), `adapters/pgvector`, `backends/{sqlite,pgvector}` (post-ADR-0022 reorg) | 14-day window: fix AGENTS.md shape section, then docs↔tree bijection gate (declared paths ↔ `ls`) goes strict |
| Stale build artifacts tracked in launcher | 2 tarballs (`engram-mcp-0.1.0.tgz`, `engramlabs-mcp-0.1.7-alpha.tgz`) vs launcher v0.6.0 | 7-day window: delete, then npm-files gate strict |
| Factory job noise (the owner's stated pain) | job `010-project-bound-branch-indexing`: **874 files / 23 MB at the job dir top level** (batch-r5-*, terra-*, glm-* logs, recovery snapshots), board still `build/running` cycle-1 round-0; `.engram-factory/tmp13` = **1.1 GB**; `tmp14-*.log` ×3; stale worktree `worktrees/009` still checked out on `factory/009-...` (job 009 shows merged) | 30-day window: (a) policy that intermediates go under `cycle-N/` or `evidence/`, never job-root; (b) tmp-dir + worktree GC rule in factory.py resume path; (c) job-root file-count gate (advisory at 50, blocking at 200) |
| Input-hash mismatches in job artifacts | 6 of 383 declared hashes (1.6%) | hash-pinning gate advisory 14 days (lets in-flight job-010 finish), then blocking |
| Uncovered acceptance criterion | job 002 `AC-013` (historical, merged) | record as accepted historical deviation, gate applies to new jobs only |
| `check-code-docs.sh` inert Rust half | gates `crates/` which doesn't exist in this layout | 14-day window: retarget to `core/ adapters/ …` or explicitly retire the Rust half |

**In-scope vs excluded for gates:** in-scope = git-tracked first-party code, docs,
contracts, `.agents/skills/`, `packages/launcher` payload, `.engram-factory/jobs/<id>/`
factory artifacts (local gate). Excluded = `vendor/` (vendored grammars/tantivy),
`*.upstream.md` (frozen upstream prose, precedent in check-docs.sh), `target/`,
`node_modules/`, `#[cfg(test)]` blocks, `.engram-factory/{worktrees,tmp*}` (GC'd, not gated).

---

### Bottom line (Q6 answer)

engram already has the two hardest rails — engine neutrality (RULE-1-shaped) and facade↔N-API
parity (registry-transport-shaped) — as fast, blocking, in-CI greps, plus a generated-artifact
drift gate (RULE-2-shaped) and near-zero RULE-3 debt. What's missing is exactly what the owner
feels: **nothing validates the SDD pipeline's own artifacts** (frontmatter, REQ↔AC↔TASK
integrity, hash pinning, ratification-before-build) and nothing prevents orphan components or
job-dir noise. Week 1 closes those with parse-class gates (<1 s each, zero new dependencies),
month 1 reaches SCCT L2, and the factory's existing provenance receipts make L3 mostly a
standardization exercise rather than new machinery.
