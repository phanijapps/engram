# Rail 07 — Zero deviations

Rule class: RULE-5 (zero-deviation mandate).
Gate: `tools/ci/check_constitution.py` (existing as of this adoption; umbrella).

## Rules

- **ZD-001 (MUST)**: `docs/constitution/DEVIATIONS.md` `## Active deviations` MUST be empty;
  any heading under Active is itself a CI failure.
- **ZD-002 (MUST)**: Every active deviation MUST record: rule ID violated, scope,
  approver (owner), opened date, expiry date (≤ 90 days), and closure condition.
- **ZD-003 (MUST)**: An expired deviation MUST be closed (resolved and moved to
  Historical) or escalated to the owner the same day; expiry without closure fails CI.
- **ZD-004 (MUST)**: Temporal clauses of rules (measured baselines with closure
  trajectories) live in `DECAY-WINDOWS.md`, never in DEVIATIONS.

## Enforcement boundary

`check_constitution.py` fails when: any heading exists under `## Active deviations`; a
deviation entry lacks approver/expiry; a decay window is past its closure date without a
closure note.

## Policy

Deviation approval is owner-only. Default maximum concurrent active deviations: 3
(exceeding it fails regardless of individual validity).

## History

- 2026-10-31: rail locked at adoption; Active section empty (day one).
