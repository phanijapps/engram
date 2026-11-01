# Tasks-template lane-coherence review

**Date:** 2026-11-01  
**Verdict:** **CHANGES REQUIRED.** The proposal has the right primary spine—`REQ → STEP → TASK → GATE → verification.md`—but AC binding is optional, build-stage re-authorship is not explained, and two sections create unowned or duplicate state. Resolve the patch list before landing the template.

## Findings and evidence

### 1. Lane closure is partial, not total

The main task table has legitimate producers: `REQ-*` comes from the spec, `STEP-*` from the plan, and its gate/evidence columns are produced by the spec AC proof table and plan `Verify (GATE)` / acceptance-to-evidence map. The proposed spec makes an AC a Gherkin acceptance contract and requires it to name a gate; the proposed plan requires every AC to have a STEP and GATE. That makes a task row a sound execution join.

However, `AC-*` is absent as a column and appears only inside the example prose in the `REQ IDs` cell (`tasks.md:31–40`). Therefore an author can produce gate-valid task rows with no AC consumer. `Done` has no stated consumer/update point. `DEV-*` and `FIND-*` are introduced only in the final prose (`tasks.md:64–68`), with no producer or defined downstream artifact. In particular, the factory records the build-stage tasks artifact *before* it creates reviewer artifacts (`factory.py:327–329, 361–390`), so “After review, record each FIND-*” cannot reliably be fulfilled in that tasks artifact. Treat delivery concerns as task-stage planning information; keep findings and their dispositions in review/decision artifacts.

### 2. REQ is load-bearing for machine totality; AC must be a first-class human binding

Keep both bindings, in separate columns. `REQ-*` is load-bearing for the current coverage-totality gate: it finds all literal `REQ-###` values in the spec and tasks and reports a missing requirement only when at least one `TASK-###` exists (`check_job_artifacts.py:53–68`). It does **not** determine whether a REQ is associated with a particular task row.

`AC-*` is load-bearing for semantic acceptance closure, not for that current regex gate. The spec says an AC is the acceptance contract and must name its proof gate; the plan maps each AC to STEP and GATE. Every spec AC should consequently occur in at least one task row, alongside the REQ and STEP that implement it. This prevents implementation work from claiming requirement coverage while silently omitting an observable Gherkin promise.

### 3. The two-author lifecycle needs an explicit re-completion protocol

At the tasks stage, `engram-tasks` completes `tasks.md`, `verification.md`, and `release.md`, with `spec.md` and `plan.md` as required direct inputs (`factory.py:327–331`). At build, `engram-builder` must submit those three documents again, now with the current source snapshot; the earlier mutable artifacts are deliberately excluded from the immutable-input check (`factory.py:344–365`). `validate_artifact` also requires the current author role, `source_snapshot`, and hash-accurate `input_artifacts` (`factory.py:94–124`).

The template currently has only the tasks-stage role and `source_snapshot: null` (`tasks.md:9–21`), and its body does not say what survives re-completion. It should state: preserve the task IDs and REQ/STEP/AC/GATE mappings and coverage claims unless a changed implementation fact requires escalation/re-plan; re-fill author role, timestamp, snapshot, input hashes, Done state, concrete paths, and links to the newly authored verification artifact. A builder must not replace planned acceptance mappings merely to make a completed build look tidy.

### 4. Gate inventory belongs in tasks; gate results belong only in `verification.md`

The task template’s “What did verification actually report?” table duplicates mutable evidence state: it has `Result` and “Raw evidence location” while pointing at `verification.md#gates` (`tasks.md:56–62`). The build flow re-authors both files together, but neither `validate_artifact` nor `check_job_artifacts.py` compares their results. They can diverge without detection.

Retain a task-side **gate inventory/handoff**—GATE ID, owning TASK, and `verification.md` anchor—because it tells the builder which evidence must be produced. Remove result/status/raw-output fields from tasks. `verification.md` must be the sole record of `NOT_RUN`, `PASS`, `FAIL`, authorized exceptions, command output, and evidence locations.

### 5. The machine contract is smaller than the proposal implies

`check_job_artifacts.py` requires a frontmatter fence and the five keys `schema_version`, `artifact`, `job_id`, `status`, and `size_tier` (`:40–48`). For cross-artifact checks it only regexes literal `REQ-###`, `STEP-###`, and `TASK-###` anywhere in document text (`:24–25, 53–68`). It checks that plan REQs exist in the spec, task STEPs exist in the plan, task REQs exist in the spec, and every spec REQ appears somewhere in tasks when a TASK exists.

It does **not** parse table columns or relationships; it does not require or validate `AC-*`, `GATE-*`, Done states, dependencies, coverage statuses, evidence references, `DEV-*`, or `FIND-*`. `FMI_RE` is declared but unused. The factory is stricter about metadata/provenance than this CI script, but likewise does not parse the task table. Keep AC/GATE and the coverage matrix because they are useful human lane contracts; do not describe them as gate-enforced. Remove the duplicate result table and undefined finding IDs because they are neither enforced nor useful at their present lifecycle point.

## Patch list

1. **Task-table header:** `| ... | STEP IDs | REQ IDs | Gate to run | Evidence reference |` → `| ... | STEP IDs | REQ IDs | AC IDs | Gate IDs | Verification handoff |`. Require every row to name all applicable literal IDs; specifically, every spec AC must occur in at least one TASK row.
2. **AC example:** `TASK-002 ... | REQ-002, AC-001 |` → place `REQ-002` in `REQ IDs` and `AC-001` in the new `AC IDs` column. This eliminates a category error and makes AC coverage reviewable.
3. **Coverage instruction:** `The matrix MUST contain every REQ-* ...` → `The matrix MUST contain every REQ-* and list its spec ACs and covering TASKs; every listed AC MUST also appear in a TASK row. Literal IDs, not ranges, are required.` Preserve REQ totality as the CI-backed claim; label AC totality as reviewer-enforced until the gate is extended.
4. **Verification section:** `## What did verification actually report?` and its `Result`/`Raw evidence location` table → `## Which gates must verification report?` with `| Gate ID | Owning TASK IDs | verification.md anchor |`; add: `verification.md is the sole mutable result record. Do not duplicate NOT_RUN/PASS/FAIL, exception, command output, or raw-evidence state here.`
5. **Lifecycle section:** add `## Build-stage re-completion` after execution order: tasks-stage author uses `source_snapshot: null` and current `spec.md`/`plan.md` hashes; build-stage author is `engram-builder`, supplies the current snapshot and required hashes, preserves IDs/mappings/coverage unless escalating a changed fact, and refreshes completion state and verification links.
6. **Delivery/findings wording:** `After review, record each FIND-* in prose ...` → `Record applicable delivery concerns and their TASK/DEV owner here. Reviewer findings and dispositions belong in the review and decision artifacts; a rework cycle may update this plan only with the corresponding approved re-plan.` Define `DEV-*` upstream or remove it from this template.
7. **Done-state wording:** add: `At tasks stage all Done boxes are unchecked. At build re-completion, the builder checks only work completed and evidenced in verification.md; incomplete work remains unchecked and triggers the stated stop/escalation path.`
