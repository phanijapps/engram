# Plan phase: one design and proof map

The supported plan format is compact-v1.

Author one plan: owner summary, chosen approach and meaningful alternatives,
bounded steps, proof map, rollback/risks and decisions/evidence. Do not restate
spec scenarios or AC definitions, add ceremonial stories, repeat review history,
maintain hash arrays, or fill an unaffected-concerns checklist. Add architecture,
interfaces, data/state flows, compatibility, migration, security and deployment
mechanisms inline when they affect this design. Scope exclusions reference the
accepted spec. Keep diagrams inline when useful or owner-requested.

The compact header has schema_version 2, plan_format compact-v1 and size_tier.
Optionally select `e2e: true` and one E2E scenario table as defined in
e2e-phase.md; the reviewed plan fixes this routing before owner Start.
Mark E2E-owned Proof gates with `E2E: E2E-001` scenario references; actual
commands live once in the E2E table. Other gates remain build checks.
Use the exact Steps and Proof table columns from the template. Step dependencies
are complete STEP IDs or none. Owned paths are explicit canonical repo-relative
paths, comma-separated; no globs/traversal/drive paths. A path is ownership scope,
not authorization to execute. Gates name runnable commands or an explicit
MANUAL: protocol with an observable expected result. Design RED/negative proof
where relevant. The controller does not execute plan commands or certify their
feasibility; independent review verifies both.

Every canonical spec requirement and AC must be covered, and each step's proof
must correspond to the requirements it claims. Read canonical compact spec
relationships; never infer them from incidental mentions. Mere coverage counts
do not prove design.

## Supporting documents only with a distinct purpose

Default: no supporting documents. Split only for a reusable/independently maintained
contract, substantial model, or genuinely separate reader need. A section inside
plan.md is sufficient otherwise. Declare optional documents once:

| Path | Purpose |
| --- | --- |
| context/interfaces.md | Reusable host-adapter contract maintained by both host teams |

Allowed paths: context/architecture.md, context/data-models.md,
context/interfaces.md, context/contracts.md. Write plain substantive Markdown,
without copied metadata, input hashes or plan prose. The orchestrator binds them
in the plan receipt. Undeclared context files fail explicitly; the controller
never deletes them or silently excludes them from review. Receipt references
ensure downstream tasks, build and owner Start consume their exact bytes.

## Preparation and independent review

The planner returns authored plan.md and only its declared supporting documents.
The orchestrator runs plan-prepare --job ID, then read-only preflight --stage plan,
review-freeze, independent reviews and complete --stage plan with plan.md followed
by declared supporting paths in table order. Preparation writes one receipt under
records/plan and leaves authored files unchanged. It is idempotent for unchanged
inputs. Receipt/spec/support/source drift invalidates readiness; pending or PASS
candidates cannot be overwritten by preparation. CHANGES permits revision within
existing review limits. No candidate is spent on a structural preflight error.

Reviewers verify mechanisms against source, actual blast radius, ownership,
contracts, failure/recovery, proof detection power and all prior findings on the
same frozen candidate. Review is semantic and independent. PASS advances to tasks;
it grants neither implementation Start nor Finish/merge authority. Unknown scope,
authority or feasibility must be resolved rather than formatted away.
