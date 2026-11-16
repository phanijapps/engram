# Spec authoring and readiness

The supported spec format is compact-v1.

## One authored source

Write spec.md from assets/templates/spec.md. Its small header declares format,
size tier, scope and reviewer applicability. Do not author hashes, job/cycle
metadata, readiness JSON, receipt files or parallel coverage tables. Use JSON
values in header fields. Optional mode full/light retains the existing policy.

The body contains:

- Outcome: user, problem and observable success.
- Scenarios: prioritized actor journeys linked to requirements and acceptance.
- Requirements: behavior and applicable constitutional rule IDs. An assessed
  absence uses `N/A: <specific scope reason>`; unknown applicability blocks ready.
- Acceptance: each AC defined once as Given/When/Then with its requirement.
  Include failure/boundary behavior or justify the exception in the design review.
  Runnable proof commands belong to the plan, not a duplicated spec test plan.
- Boundaries: scope, exclusions/authority, affected interfaces and compatibility.
- Decisions and evidence: address every brief LED, source/revision, assumptions
  and consequences of material decisions. Add NFRs or entity details only where
  this change needs them. Keep resolved review history in review receipts.

IDs remain stable. Tables use the exact headers in the template; use `\|` for
a literal pipe inside a cell. REQ/AC references are complete IDs, separated by
commas or spaces. Do not duplicate acceptance definitions in other sections.
Source pointers support claims; hashes belong to machine receipts. Methodology,
worked examples and writing instructions do not belong in delivered specs.

## Multi-surface scope

Set scope to multi-surface. Add ONE Surfaces table, replacing separate inventory
and coverage representations. It records classification and owner authority as
well as navigation/states for in-scope surfaces. Use these columns:

| ID | Capability | Classification | Authority | Entry | Navigation | Scenario | Requirements | Acceptance | Normal | Empty | Loading | Error | Unavailable | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |

Allowed classification: in-scope, support, infrastructure, conditional, out-of-scope.
Use `-` for cells not used by a non-in-scope row. Conditional/excluded rows need
an owner decision reference. In-scope rows require navigation, scenario, REQ/AC
links and each state as AC IDs or `N/A: <specific reason>`. Missing/unknown is
not N/A. Add a second row/ID if a different journey has distinct coverage.

Add one Review conditions table with columns Condition and Evidence or protocol.
Use five rows: Viewports and browser, Themes, Accessibility, Visual judgment,
Reference evidence. State actual inspected sources and unavailable inspection;
do not invent visual parity. These conditions are authored once, not retyped in
frontmatter. Independent review compares the inventory against actual source,
owner scope and relevant negative states; row presence cannot prove completeness.

## Controller preparation and review

The analyzer returns authored spec.md. The orchestrator runs:

`factory.py --repo REPO spec-prepare --job ID`

Preparation validates content, derives the readiness/rule index in memory, and
writes `records/spec/receipt.json` containing the authored spec hash, source base,
job/cycle identity and actual accepted input hashes. It does not rewrite spec.md,
ratify the spec or claim semantic review. Repeat preparation on changed draft
bytes before review, not after PASS. The command is idempotent for unchanged
inputs. The author never edits receipt JSON or computes hashes manually.

Then run read-only `preflight --job ID --stage spec`, freeze, independent review
and completion. Receipt and authored spec are frozen and archived together.
Changed authored bytes require preparation; changed frozen bytes cannot inherit
a PASS. Existing review limits, error recovery, independent judgment and later
exact-plan Start/Finish gates remain in force.

All structural checks have limits: they
cannot establish source completeness, owner authority, requirement quality or
semantic coverage merely because IDs and reasons exist.
