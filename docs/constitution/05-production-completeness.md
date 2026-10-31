# Rail 05 — Production completeness

Rule class: RULE-3 (zero stubs/placeholders/mocks reachable from production).
Gate: `tools/ci/check_production_completeness.py` (planned; baseline measured 2026-10-31).

## Rules

- **PC-001 (MUST)**: Production-reachable code MUST NOT contain `TODO`, `FIXME`, `XXX`,
  "coming soon", placeholder text, `todo!`/`unimplemented!`, or mock data. Test fixtures
  live in test directories only.
- **PC-002 (MUST)**: SDD artifacts are governed too: ratified sections of factory job
  artifacts (brief, analysis/spec, plan, tasks, verification, release) MUST NOT contain
  `TBD`/`TODO` stub cells — an unratified draft MAY, a ratified artifact MUST NOT.
  Enforced by `tools/ci/check_job_artifacts.py`.
- **PC-003 (SHOULD)**: Doc-level `TODO` markers SHOULD stay zero in tracked docs
  (already enforced today by `check_docs.py`).

## Enforcement boundary (measured baseline 2026-10-31)

`rg -c 'TODO|FIXME|todo!|unimplemented!' core adapters bindings mcp codegraph` →
**0 markers** in first-party Rust. The Rust side can be enforced near-strict immediately;
the marker gate ships as blocking for those trees once `check_production_completeness.py`
lands (month 1). SDD-artifact stub checking (PC-002) ships now with
`check_job_artifacts.py` (advisory → blocking after one clean cycle).

## Deviation path

A stub on a production path requires a DEVIATIONS entry with a removal date; the
zero-deviation gate failure stands until closed.

## History

- 2026-10-31: rail locked at adoption; baseline recorded (0 markers, first-party Rust).
