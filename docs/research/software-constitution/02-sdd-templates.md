# SDD template redesign: traceable intent without ceremonial repetition

**Date:** 2026-10-31  
**Scope:** Q2–Q5 — brief/intent, spec (`analysis.md`), plan, and tasks. This is a redesign proposal only; it does not change factory assets or skills.

## Method and evidence

I fetched and read [Software Constitution principles](https://softwareconstitution.com/principles) and its [SPEC.md](https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md). The central applicable rule is not “better prose”: a rule needs a canonical declaration and a merge-blocking gate. In particular, SPEC §RULE-3 rejects production-reachable placeholders, §RULE-4 requires canonical evidence events declared before use, §RULE-5 makes active deviations a failing condition, and §RULE-6 rejects orphaned declared components.

I read all nine current factory templates (62–150 lines), both driver skills, and these real jobs: 002 (`brief` 123, `analysis` 187, `plan` 144, `tasks` 149 lines; plus its cycle-1 history), 003 (118/197/141/127), 006 (166/111/139), 007 (158/150/112), and 010 (96/331/412/273). I also inspected raw asks, cycle/history directories, and review reports.

Index mining ran before writing:

- `engram-mcp recall "factory job rework noisy template" ...` → `hit_count: 0`; **index empty for `factory job rework noisy template`**.
- `engram-mcp recall "light team mode factory" ...` → `hit_count: 0`; **index empty for `light team mode factory`**.

Those are the two actual index results; repository artifacts below are the evidence basis.

## Diagnosis

### What works and must be kept

- **Brief authority is valuable.** The current brief preserves verbatim asks, MoSCoW, boundaries, decision authority, and owner ratification. Job 002’s ledger makes “`.agents/skills` remains the single source” explicitly delegated (`jobs/002.../brief.md:73-80`), rather than silently turning an owner suggestion into a requirement. Keep MoSCoW and the decision ledger.
- **Analysis has strong ingredients.** Its EARS requirements, evidence labels, acceptance table, explicit scope, and negative verification stance are the right concepts (`assets/templates/analysis.md:84-116`). Job 007 is a good small-job example of measurable constraints: “exactly one path” and “no lint escapes” (`jobs/007.../analysis.md:68-90`).
- **Plan and task artifacts already try to link rather than restate.** The plan says requirements belong in analysis (`assets/templates/plan.md:26-28`), and tasks says acceptance lives in analysis (`assets/templates/tasks.md:12-16`). Keep that ownership rule, but make it mechanically true.

### Why small jobs are noisy and slow

1. **The same full ceremony is required even when the change is one file.** Job 007’s raw ask is a one-file, eight-lint cleanup, yet its analysis/plan/tasks total **420 lines**. Its tasks spend four delivery-checklist lines on “N/A” explanations (`jobs/007.../tasks.md:82-100`), after the plan already establishes “one file” and no API/schema surface (`jobs/007.../plan.md:97-103`). Job 006 similarly lacks a registered brief (`jobs/006.../analysis.md:25-28`) but still carries a 139-line tasks artifact, including generic N/A delivery rows (`jobs/006.../tasks.md:110-128`).
2. **Template sections invite defensive prose rather than information.** Current analysis requires story, evidence, decisions, requirements, acceptance, scope, verification, reviewer applicability, risks, glossary, and environment inventory; current tasks requires execution rules, delivery checklist, rework tracking, glossary, and profile prose. The templates budget narrative separately from tables, so authors move duplicated detail into tables. Job 003 calls its analysis “~950” words against a 600 budget because the ledger, six decisions, and eleven requirements are exempt (`jobs/003.../analysis.md:22-23`). Job 010 then grows to 331/412/273 lines while repeatedly declaring its excess “necessary” (`jobs/010.../analysis.md:22`, `plan.md:20`, `tasks.md:19-20`).
3. **Artifacts are written before their stable facts exist.** Tasks are written before build and review but must contain a rework table. It necessarily starts as filler: job 006 says “N/A — no review round exists yet” (`jobs/006.../tasks.md:122-128`); job 003 says the same (`jobs/003.../tasks.md:104-110`). A future review cannot be truthfully documented at task-authoring time. Move rework history to a generated review ledger or make it post-review metadata, not a required pre-build section.
4. **The title repeats the raw ask through every artifact.** Job 006’s long remediation statement appears as plan title (`plan.md:15`) and tasks title (`tasks.md:21`), despite the feature/brief being authoritative. Titles should be short identifiers and links; the source ask has one canonical home.

### Duplication trace: one job-002 requirement

Trace the cycle-1 “authored skills are the sole source and future skills ship without lists” requirement.

| Artifact | Restatement(s) | Count |
|---|---|---:|
| Brief | Outcome says all six/future skills flow; constraint says “single authored source”; LED-1 and LED-2 repeat both (`brief.md:31-80`). | 4 |
| Analysis | Intent, LED table, story, DEC-001/002, REQ-002/003, AC-002/003, scope, and glossary restate it (`history/cycle-1/analysis.md:24-143`). | 8 |
| Plan | Approach, DEC-P01/P03, steps 1/4/5/9/10, contracts, verification map, rollback, glossary repeat it (`history/cycle-1/plan.md:23-167`). | 10 |
| Tasks | Task 1/4/5/9/10/12, coverage, negative/export coverage, checklist, profile, and handoff repeat it (`history/cycle-1/tasks.md:35-172`). | 10 |

That is **at least 32 conceptual restatements** of one requirement. Some are useful evidence; most are copied implementation narration. The replacement must preserve links and evidence while making the requirement sentence canonical in the spec only.

### What is under-specified and causes rework

The current templates have prose conventions, not enforceable identities for rules, steps, gates, files, or coverage. That permits real gaps:

- Job 003 reviewers found that the analysis template’s hardcoded `mode: "full"` can silently change a reworked light job; they asked for a rendered value or no default (`jobs/003.../cycle-1/round-1/review-code.md:55-56`, `review-data.md:41-42`). The template had no typed “authority input / generated default / owner pin” declaration to gate.
- Job 002’s second cycle fixed a destination-link defect, yet review found a new ordering defect and two additional low security defects (`jobs/002.../cycle-2/round-1/review-code.md:43`; `review-security.md:38-39`). The task table linked requirements, but did not require a rule-to-step-to-gate chain or an explicit negative-case row for every changed write path.
- Job 010’s current task file is a 273-line mutable execution log, with status mixing proof, blocker, authorization, and future tasks (`jobs/010.../tasks.md:19-20`). Its rework ledger begins at line 222. This is evidence that tasks need structured state, not more narrative sections.

### Missing machine-gate inputs

A CI gate cannot reliably infer from prose whether a requirement is covered, a path is real, a named command ran, an exception expires, or a decision is authorized. The artifacts need: stable IDs; typed status; explicit rule IDs; exact paths; named gate IDs; command/result references; owner/expiry for deviations; and total coverage matrices. These are the fields that allow a repository to apply the Software Constitution’s RULE-3/4/5 model to its own SDD process.

## Common conventions for all four artifacts

- Files remain Markdown-first, with YAML frontmatter. A JSON Schema validates frontmatter and compact table/list grammars; Markdown remains the human explanation.
- IDs are immutable within a job/cycle: `LED-001`, `EVD-001`, `DEC-001`, `REQ-001`, `AC-001`, `STEP-001`, `TASK-001`, `GATE-001`, `RISK-001`, `DEV-001`.
- `REQ` is the only canonical statement of behavior. `AC` proves a REQ; a step implements REQs; a task executes a step; a gate supplies evidence. Consumers link IDs and never copy REQ text.
- `RULE-*` references must resolve to the repository constitution. `GATE-*` references must resolve to a declared CI gate registry. `path` values are repository-relative and must exist unless explicitly marked `new-path` in a plan step.
- Ratified/ready artifacts reject `TBD`, `TODO`, `FIXME`, `XXX`, `N/A` without `reason`, `[placeholder]`, and unresolved template tokens. This is the artifact analogue of RULE-3; it does not ban those literal strings in quoted evidence.
- A deviation is never a casual “N/A”: it is a `DEV-*` record in `DEVIATIONS.md` with owner, scope, expiry, and compensating gate. Active deviations fail under RULE-5 unless the constitution’s explicit policy permits a time window.

## Template 1 — Brief (intent framing)

```md
---
schema_version: 2
artifact: brief
job_id: JOB-ID
status: draft # draft | ratified
size_tier: S # S | M | L
owner: OWNER-REF
ratified_at: null
constitution_rules_touched: [] # e.g. [RULE-4]
---
# Brief: short owner-facing outcome

## Ask (verbatim)
> Owner words only. The feature file links here; do not paraphrase elsewhere.

## Problem, not symptom
- **Problem:** affected user/system and harmful outcome.
- **Observed symptom/evidence:** concrete signal; do not prescribe a mechanism.
- **Why now:** consequence of not acting.

## Intended outcome
- OUT-001: observable owner success signal and judgment moment.

## Priorities (MoSCoW)
| Priority | Outcome/boundary | ID |
|---|---|---|
| Must | behavior required for OUT-001 | MOS-001 |
| Should | in-scope behavior, not required for success | MOS-002 |
| Could | follow-up only | MOS-003 |
| Won't | explicit non-goal | MOS-004 |

## Constraints and constitution impact
| Constraint or rule | Source/authority | Effect |
|---|---|---|
| RULE-004 | constitution path/section | rule touched or `none` |

## Decision ledger
| ID | Choice | Authority | Bound decision / verbatim owner statement | Revisit trigger |
|---|---|---|---|---|
| LED-001 | consequential choice | HUMAN-DECIDED | message reference and quote | trigger |

## Scope
- **In:** outcome IDs/MoSCoW IDs.
- **Out:** explicit exclusions.
- **Open:** `none`, or LED IDs that block ratification.

## Owner ratification
| Owner reference | Date | Verbatim confirmation |
|---|---|---|
| channel/message ID | YYYY-MM-DD | quote |

## Handoff
- Feature source: `path` and content hash.
- Analysis may decide: only explicitly delegated LED IDs and boundaries.
```

**Kept:** verbatim ask, MoSCoW, decision ledger, explicit authority, and ratification. **Changed:** problem is separated from symptom; `size_tier` and `constitution_rules_touched` become typed fields. S jobs need not invent a glossary, environment inventory, or speculative delivery checklist to earn a correct brief.

## Template 2 — Spec (renamed from `analysis.md`)

```md
---
schema_version: 2
artifact: spec
job_id: JOB-ID
cycle: 1
status: draft # draft | ready | superseded
size_tier: S
brief_ref: { path: brief.md, sha256: SHA256 }
constitution_rules_touched: [RULE-004]
required_reviewers: [code, security, platform]
---
# Spec: short outcome

## Intent trace
| Brief ID | Interpretation / disposition | Authority |
|---|---|---|
| MOS-001 | included as REQ-001 | HUMAN-DECIDED |
| LED-001 | resolved by DEC-001 | DELEGATED |

## Evidence
| ID | Claim | Status | Citation |
|---|---|---|---|
| EVD-001 | one falsifiable claim | proven | `path/to/file:12-18` |

## Decisions
| ID | LED / authority | Decision | Rejected alternative | Consequence |
|---|---|---|---|---|
| DEC-001 | LED-001 / DELEGATED | one choice | one alternative | observable effect |

## Requirements (canonical behavior)
| ID | Priority | Testable statement | Rules | Evidence |
|---|---|---|---|---|
| REQ-001 | MUST | When trigger, the system shall observable response. | RULE-004 | EVD-001 |

## Acceptance and named gates
| ID | REQ IDs | Given / When / Then | Gate IDs | Negative case |
|---|---|---|---|---|
| AC-001 | REQ-001 | concise executable behavior | GATE-001 | denial/failure behavior |

## Scope boundaries
- **In:** named REQ IDs and paths/areas.
- **Out:** named behavior/paths; `none` is valid.
- **Deferred:** DEV IDs or follow-up owner + trigger; never an unnamed omission.

## Verification stance
| Gate ID | Command or fixture | Proves AC IDs | Expected evidence |
|---|---|---|---|
| GATE-001 | `command` | AC-001 | exit/result/artifact |

## Risks and reviewer applicability
| ID | Risk / reviewer discipline | Trigger | Owner |
|---|---|---|---|
| RISK-001 | scenario / code=yes | signal | role |

## Terms and environment (only when needed)
- TERM-001: definition that disambiguates a requirement.
- Environment: platform/version only if it changes verification or behavior.
```

**Kept:** evidence labels, decisions, EARS-style requirements, acceptance, explicit out-list, negative evidence, and reviewer applicability. **Changed:** the requirements table is the canonical behavior source; acceptance is explicitly bound to gate IDs; evidence must use `file:line`/command citations; scope is an explicit out-list rather than prose distributed through profile sections. `NFR-*` is removed: measurable behavior is `REQ-*`, everything else is a risk, constraint, or gate.

## Template 3 — Plan

```md
---
schema_version: 2
artifact: plan
job_id: JOB-ID
cycle: 1
status: draft # draft | ready
size_tier: S
spec_ref: { path: analysis.md, sha256: SHA256 }
constitution_rules_touched: [RULE-004]
---
# Plan: short outcome

## Approach
One concise design narrative. Link consumed `DEC-*`; do not restate `REQ-*`.

## New decisions
| ID | Authority / rule | Decision | Why now | Consequence |
|---|---|---|---|---|
| DEC-002 | DELEGATED / RULE-004 | choice | new fact | effect |

## Steps
| ID | Implement | REQ IDs | Rule IDs | Concrete paths | Verify (GATE-ID / command) | Rollback |
|---|---|---|---|---|---|---|
| STEP-001 | imperative action | REQ-001 | RULE-004 | `src/a.rs`; `new-path: tests/a.rs` | GATE-001 | exact revert/recovery |

## Contracts and compatibility
| Surface | Classification | REQ IDs | Consumer / migration / documentation action |
|---|---|---|---|
| public/internal/none | compatible/breaking/none | REQ-001 | concrete action |

## Blast radius
| Area or symbol | Concrete dependent paths/count | Risk | Owning STEP IDs |
|---|---|---|---|
| area | paths or measured count | failure shape | STEP-001 |

## Verification map
| AC ID | Step IDs | Gate IDs | Omission / deviation |
|---|---|---|---|
| AC-001 | STEP-001 | GATE-001 | none |

## Rollback and recovery
- STEP-001: exact rollback command/action and partial-failure state.
```

**Kept:** approach, new decisions, steps, compatibility, blast radius, verification map, and rollback. **Changed:** every step must carry REQ IDs, rule IDs, concrete paths, proof, and rollback; no orphan step is valid. “Blast radius” must be paths/counts, not “bounded”; the job-006 style measured dependency statement is useful, while generic profile prose is not.

## Template 4 — Tasks

```md
---
schema_version: 2
artifact: tasks
job_id: JOB-ID
cycle: 1
status: draft # draft | ready | executing | complete
size_tier: S
plan_ref: { path: plan.md, sha256: SHA256 }
---
# Tasks: short outcome

## Executable task list
| Done | ID | Imperative outcome | STEP IDs | REQ IDs | Gate to run | Evidence reference |
|---|---|---|---|---|---|---|
| [ ] | TASK-001 | one independently verifiable action | STEP-001 | REQ-001 | GATE-001 | `verification.md#gate-001` |

## Ordering and stop conditions
- TASK-001 depends on: `none`.
- Stop/escalate on: named risk, failed gate, or required deviation.
- Commit/release rule: only when the plan requires it; do not impose one-commit-per-task by default.

## REQ × task coverage matrix
| REQ ID | TASK IDs | AC IDs | Coverage status |
|---|---|---|---|
| REQ-001 | TASK-001 | AC-001 | total |

## Gate inventory
| Gate ID | Owning TASK | Status | Raw evidence location |
|---|---|---|---|
| GATE-001 | TASK-001 | NOT_RUN | `verification.md#gate-001` |

## Delivery applicability
| Concern | Applies? | TASK/DEV ID or reason |
|---|---|---|
| migration | no | no persisted format changes |
| authorization/denial | yes | TASK-001 |
| release/operator handoff | no | no runtime surface |

## Review-finding links (post-review only)
| Finding ID | Disposition | TASK/DEV ID | Evidence |
|---|---|---|---|
| none yet | pending review | — | — |
```

**Kept:** checkbox-style task list, explicit dependencies, evidence commands, coverage, and delivery considerations. **Changed:** make checkboxes and identifiers first-class; map tasks directly to plan steps and requirements; retain a total REQ×task matrix; move mutable review history into a post-review ledger rather than forcing false `N/A` rows at authoring time. The task template does not repeat acceptance prose, plan approach, or full-stack boilerplate.

## Tiering: the direct noise reduction

| Artifact / section | S — localized, reversible, one surface | M — several paths or one boundary | L — cross-store/transport/destructive or breaking |
|---|---|---|---|
| Brief | Required: ask, problem/symptom, outcome, MoSCoW, ledger, scope, tier. Optional: rules if none. Omit separate suggestions section when none. | All S, plus constraints/rules and ratification detail. | All M; explicit destructive/compatibility authority required. |
| Spec | Required: intent trace, evidence, REQ/AC/gate tables, out-list, risks/reviewers. Optional: terms/environment only if behavior changes. | All S; decisions and compatibility when applicable. | All M; environment, transport/backend parity, recovery and retention evidence required. |
| Plan | Required: approach, step table, verification map, rollback. Optional: compatibility/blast radius when no public or dependency impact. | All S; concrete contracts and measured blast radius. | All M; every transport/store boundary, staged recovery, and named destructive authorization. |
| Tasks | Required: checkbox task list, ordering, REQ×task matrix, gate inventory. Optional: delivery applicability rows that are `no` with one reason. Omit review ledger until review exists. | All S; delivery applicability and release handoff if applicable. | All M; task ownership by component, real-store matrix, review-finding ledger, and reset/recovery gates. |

A tier is selected in the brief and may only increase with a linked decision/rework event. It is not a license to omit required behavior, authorization, or proof.

## Gate-ability checklist

| Artifact | CI can mechanically verify | Constitution linkage |
|---|---|---|
| Brief | Frontmatter schema; tier is S/M/L; ratified briefs have owner reference/date/quote; LED IDs unique; each delegated row has boundary and revisit trigger; no unresolved Open; all declared `RULE-*` resolve. | RULE-3 rejects ratified placeholder/Open markers; RULE-4 treats ratification and ledger IDs as canonical authority events; RULE-5 rejects undeclared active exceptions. |
| Spec | Schema; evidence citations match `path:line` and paths exist; every LED/MoSCoW item has disposition; unique REQ/AC/GATE IDs; every REQ has AC and gate; every AC names ≥1 REQ and gate; explicit out-list present; required reviewers match applicability. | RULE-3 blocks stubs; RULE-4 makes spec IDs/citations the canonical requirement/evidence registry; RULE-5 requires any omission to reference an expiring deviation. |
| Plan | Every `STEP-*` has ≥1 REQ, rule, existing/new path, gate, and rollback; every REQ has a step; paths exist or are marked `new-path`; every AC appears in verification map; no unknown REQ/RULE/GATE references. | RULE-4 verifies canonical-first rule/gate declarations and referential integrity; RULE-6 rejects orphan steps/paths. |
| Tasks | Checkbox/ID grammar; each task maps to existing step and REQ; each task has a gate; every REQ has one or more tasks and `total` coverage; every planned gate starts `NOT_RUN`; no pre-review finding claimed resolved; execution status can change only with verification evidence. | RULE-3 blocks fake PASS/TBD; RULE-4 makes gate result/evidence the canonical event; RULE-5 requires a DEV reference for any incomplete required coverage. |

### Suggested gate order

1. `validate-sdd-frontmatter` — JSON Schema and IDs.
2. `validate-sdd-references` — LED→DEC, REQ→AC→STEP→TASK→GATE totality and path resolution.
3. `validate-sdd-placeholders` — ratified/ready artifact RULE-3 scan, excluding template assets and quoted source blocks.
4. `validate-sdd-evidence-events` — gate registry and verification result references are canonical-first (RULE-4).
5. `validate-sdd-deviations` — `DEVIATIONS.md` active section and every `DEV-*` expiry/owner (RULE-5).

This evolves the existing information architecture rather than replacing it: Markdown remains authored by agents and reviewed by people, while IDs and frontmatter give CI the minimal facts required to reject boilerplate, orphaned work, unverifiable claims, and silent exceptions.
