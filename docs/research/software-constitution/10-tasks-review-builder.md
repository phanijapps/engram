# Tasks-template review — builder-workflow lens

Date: 2026-11-01  
Verdict: **fix-then-ship**. Keep the executable checklist and code-based acceptance doctrine; repair the worked example, completion semantics, and duplicated evidence before adoption.

## Scope and evidence

Read the brief and all five requested files. References below are 1-based physical lines: **P** = `docs/research/software-constitution/proposed-templates/tasks.md`; **J** = `.engram-factory/jobs/011-en004-fail-closed/tasks.md`; **L** = `.agents/skills/engram-factory/assets/templates/tasks.md`. Also read `05-spec-craft-analysis.md` (§Tasks principles and real-output contrast) and `07-comparative-verdict.md` (§Merge criteria and worked micro-example). Scores are prospective builder assessments, not measured productivity; 5 is best. No source, template, job, or gate was edited. No index tooling was used, as requested.

## Scored findings

### 1. Executability — 3/5

**Direct evidence:** P:31–34 gives bounded outcomes, STEP/REQ links and a fixture path, an improvement over J:22–23's “two-case unit test committed.” P:44–46 provides dependencies, escalation and a sensible plan-dependent commit rule. However, P:31–34 only names GATE-001, not its command or expected result; P:58–62 defines gate results but never defines when `[ ]` becomes `[x]`. The shared `verification.md#gates` anchor does not identify a particular run. P:29 requires a pre-fix failure, while P:44 orders the fixture after the fix without explaining how to recover the baseline.

**Builder interpretation:** not executable literally in isolation; usable with resolvable plan/spec gate definitions, but the template must explicitly require those links. The stop rule is useful, not bureaucracy, once it distinguishes an expected baseline failure from an unexpected failure. Otherwise an intended red test could be treated as a blocker, or silently skipped.

**Fix:** define completion as the bounded outcome plus recorded required-gate evidence; exceptions require explicit authority, not an implied PASS. Require resolvable commands, expected outcomes and exact run anchors. Allow fixture-first execution or a named isolated pre-fix baseline; identify who/what unblocks escalation.

### 2. Coverage matrix — 2/5; recommend **derive-in-gate**

**Direct evidence:** P:50–54 requires a second hand-maintained matrix and supplies only REQ-001 although P:34 introduces REQ-002. P:34 puts AC-001 in a column labeled “REQ IDs,” while P:52 assigns AC-001 only to TASK-001. J:20–23 and J:31–34 demonstrate that repeated mappings can look total while obscuring what the fixture actually proves. Merge criterion 4 in `07-comparative-verdict.md` expressly permits compact mappings or generated views, rather than a mandatory matrix.

**Builder interpretation:** coverage earns its existence; this second editable representation does not. The current TASK rows are not sufficient to derive AC coverage reliably because their columns do not consistently model AC links. Merely deleting the matrix now would lose information.

**Fix:** make TASK rows the canonical STEP/REQ/AC/gate/fixture links, then have a gate derive coverage against defined upstream IDs, reject dangling or missing links, and require authorized exceptions for uncovered obligations. Until that gate exists and is tested, retain one compact explicit mapping; do not claim derivation or semantic validation already works. Human review must still judge whether each linked fixture proves its AC.

### 3. Test-scenarios-as-code doctrine — 4/5

**Direct evidence:** P:36–40 explicitly teaches ordinary TASK rows bound to AC, gate and fixture path, with no separate scenario document. P:34 demonstrates the path and two concrete cases. This is the strongest improvement. But the example mixes REQ and AC types, leaves TASK-001 without an AC link, and does not demonstrate a complete requirement-to-proof mapping. The comparative verdict's worked example adds a present-path violation case and warns that existing scan tests do not automatically prove CLI exit behavior.

**Builder interpretation:** the doctrine is clear; the example is not yet trustworthy enough to copy. Two cases establish only their stated observations, not universal healthy-output compatibility or CLI behavior. The test implementation itself was outside this review's listed files, so the earlier report's warning is not a newly reproduced test result.

**Fix:** give AC links their own column, map the fixture to the requirements it actually proves, and name the asserted boundary. If the inherited requirement includes preserved violation detection or CLI exit behavior, add those fixture instances and an appropriate command; otherwise narrow the proof claim. Do not invent AC IDs without checking the spec.

### 4. Delivery concerns and review findings at S-tier — 4/5

**Direct evidence:** P:66–68 replaces L:53–65's default “no” applicability table and dummy finding row with prose, but P:67 still offers three stock non-applicability explanations and P:68 still requires a pre-review “no findings yet.” L:19–21 already permits S-tier delivery omission when nothing applies.

**Builder interpretation:** these sections earn space only when they contain work, authority or findings. Migration, authorization and handoff can matter even in tiny jobs; silently ignoring an applicable concern is not acceptable. Requiring boilerplate denials and a pending-review sentence does not improve that assessment.

**Fix:** use mandatory-when-applicable one-liners with TASK/DEV owners. Omit empty delivery and review headings at S-tier; once findings exist, disposition links are mandatory. Do not treat absent findings as proof that review happened; review completion belongs to its authoritative record.

### 5. Friction / deletions — 3/5

**Direct evidence:** P:28–29 asks for a delivery argument and supplies an example; P:44–45 repeats ordering and stopping. P:50–54 duplicates linkage, P:58–62 introduces another editable result table, and P:67–68 invites N/A and pending-review filler. J:20–23 names only GATE-001 while J:38–41 also lists GATE-002: an observed bookkeeping inconsistency, not evidence that a test failed.

**Builder interpretation:** the proposal is shorter in spirit, but asks a builder to maintain overlapping representations of both obligations and state. The obvious process risk is drift, not insufficient prose.

**Fix / delete:** remove the mandatory delivery-argument paragraph for S-tier; retain at most a one-sentence execution constraint when useful. Delete the manual coverage table after validated derivation is available. Replace the local result inventory with references to canonical verification results. Delete the three default N/A examples and pre-review filler. Keep dependencies, stop conditions, stable IDs, and machine-readable frontmatter.

### 6. Missing builder support — 3/5

**Direct evidence:** P:44–46 covers dependency and commit policy but provides no blocked-state or resume record. P:58–62 points toward verification without assigning who records results or how failures, reruns and exceptions remain discoverable. The tasks principles in `05-spec-craft-analysis.md` call for truthful planned/executed/verified state, preserved failed evidence, and reconciled inventories.

**Builder interpretation:** no diary or mandatory per-task commit log is needed. The missing minimum is a resumable blocker and an explicit verification handoff. Completion must not mean that an unchecked gate is someone else's invisible problem.

**Fix:** add an optional blocked-task line containing TASK, blocker, owner, unblock condition and evidence. State that the builder appends exact commands, expected/actual outcomes and run evidence to `verification.md`, retaining prior failures; tasks link to that record. Ready-for-verification means implementation is complete and required build checks are recorded, not that later independent verification is already PASS. Keep the existing plan-dependent commit rule unchanged.

## PATCH LIST — exact replacements / insertions

1. **P:31 header** `| Done | ID | Imperative outcome | STEP IDs | REQ IDs | Gate to run | Evidence reference |` → `| Done | ID | Imperative outcome / fixture path | STEP IDs | REQ IDs | AC IDs | Gates to run | Evidence reference |`; update divider and all rows. Move AC-001 out of TASK-002's REQ cell into AC IDs; populate remaining links from the spec rather than guessing.
2. **P:28–29 delivery-argument prompt and example** → `For S-tier, start with the task list; add one execution-constraint sentence only when it changes how work proceeds.`
3. **Insert after task table:** `Keep [ ] until the bounded outcome is implemented and every required gate has recorded evidence. Use [x] only for PASS or an explicitly authorized exception linked here; an exception is not PASS. Every gate ID must resolve to a runnable command/fixture and expected result in the spec or verification record.`
4. **P:44 dependency sentence** → `State each dependency or none. For regression tests, name the isolated pre-fix baseline or run the fixture first; record expected baseline failure separately from unexpected gate failure.`
5. **P:45 stop sentence** → `Stop on an unexpected required-gate failure, unresolved authority, or required deviation; record TASK, blocker, owner, evidence and the condition/decision that permits resumption. Do not broaden scope silently.`
6. **P:48–54 coverage section** → `## Coverage\nTASK rows are the canonical links. Until semantic coverage validation is implemented and tested, keep one compact REQ→AC→gate and REQ→STEP→TASK mapping, with owner/trigger exceptions. Once validated derivation is available, link its report instead of maintaining a second table.`
7. **P:56–62 result section** → `## Verification handoff\nThe builder records commands, expected and actual outcomes, status and raw run evidence in verification.md; TASK rows link exact gate/run anchors such as verification.md#gate-001-run-1. Preserve failed and superseded runs. Unexecuted gates remain NOT_RUN. Independent verification is a later judgment, not implied by task completion.`
8. **P:64–68 delivery/review section** → `## Applicable delivery work and review findings\nOnly when applicable, add a one-line migration, authorization/denial or release/handoff obligation linked to TASK/DEV. After findings exist, link each FIND to its disposition, TASK/DEV and evidence. Omit empty sections for S-tier; absence is not evidence of completed review.`
9. **P:34 worked fixture outcome** `Author the fixture proving AC-001: two concrete test instances (missing-path fails named; present-path clean) in tools/ci/tests/test_engine_neutrality.py.` → `Implement the spec's named AC fixtures in tools/ci/tests/test_engine_neutrality.py; assert missing-path diagnostics and clean-path behavior at the declared boundary, plus preserved violation detection/CLI outcomes wherever required by the linked ACs. Record baseline and fixed-run evidence.` Remove copied backticks as needed when applying inside the Markdown cell; resolve AC IDs and commands before adoption.

## Validation and residual risks

Manual validation: inspected all requested documents and re-read proposal, real-output and live-template line ranges to verify citations. No commands or tests ran; this is review-only, not proof of validator compatibility. Residual risks: proposed column changes and derived coverage require coordinated parser/gate fixtures and backward-compatibility review; task and gate IDs still require reconciliation with the canonical spec/plan; no empirical builder timing was measured. Only this review artifact was written.