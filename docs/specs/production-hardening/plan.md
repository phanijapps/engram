# Plan: Production Hardening

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Start with the public-repository hygiene needed before outside contributors
arrive: accurate status, governance, and release gates. Do not claim security,
performance, or release maturity that is not backed by automation and evidence.

## Constraints

- Documentation must not conflict with ADR-0004 compatibility rules.
- Release instructions must be checklists, not unpublished automation claims.
- Governance must be lightweight enough for a maintainer-led early project.

## Construction tests

**Documentation checks:** docs hook, markdown grep for stale pre-implementation
language, and full repository gates.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No domain or API contract changes.

### Component / module decomposition

- `README.md` owns public project status and validation quick path.
- `GOVERNANCE.md` owns maintainer decision rules.
- `docs/release-checklist.md` owns release gates and claim restrictions.

### Failure, edge cases & resilience

Docs should be explicit that passing CI is not a security audit, benchmark, or
production-readiness claim.

## Tasks

### T1: Public repository hygiene

**Depends on:** existing CI and contributor docs.

**Tests:**
- README no longer says Engram is pre-implementation.
- Governance and release checklist are linked.
- Validation gates still pass.

**Approach:**
- Update status docs.
- Add governance and release checklist.
- Update roadmap/changelog.

**Done when:** docs and workspace gates pass or known local-doc-hook caveats are
documented.

## Rollout

Documentation and repository metadata only.

## Risks

- Overstating readiness can mislead users; keep release claims conservative.

## Changelog

- 2026-06-29: initial public-repository hygiene plan.
- 2026-06-29: shipped README status, governance, and release checklist updates.
