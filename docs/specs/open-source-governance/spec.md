# Spec: Open Source Governance

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0001, ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** governance

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has a top-level `GOVERNANCE.md` that explains how maintainers make
decisions, how contract changes are reviewed, how conflicts are resolved, and
how release authority works while the project is pre-1.0.

## Boundaries

### Always do

- Keep governance aligned with contract-first development.
- Link decision paths to ADRs, RFCs, specs, and release gates.
- Keep maintainership language practical for a small pre-1.0 open-source
  project.
- Keep conduct, security, and contributing policies in their existing files.

### Ask first

- Create a legal foundation, steering committee, or elected governance model.
- Publish personal maintainer contact information.
- Change license, code of conduct, or security reporting policy.
- Claim production readiness or project maturity beyond pre-1.0.

### Never do

- Let governance override accepted contract compatibility rules.
- Put vulnerability reports or conduct investigations in public issue flow.
- Treat roadmap entries as accepted contracts without a spec and implementation.
- Add runtime code, dependencies, or generated artifacts for this phase.

## Testing Strategy

- Goal-based: docs checks prove the new governance document is tracked and
  links resolve under the repository documentation hooks.
- Regression: existing contribution, security, release, and code-of-conduct
  docs remain the source of truth for their own policy areas.

## Acceptance Criteria

- [x] `GOVERNANCE.md` exists at repository root.
- [x] Governance documents maintainer decision flow for specs, ADRs, RFCs,
  contracts, releases, disputes, and security/conduct escalation.
- [x] `README.md` and `CONTRIBUTING.md` references to `GOVERNANCE.md` resolve.
- [x] No contract, schema, runtime code, or generated TypeScript changes are
  introduced.
- [x] Documentation checks pass.

## Assumptions

- Technical: `README.md` already references `GOVERNANCE.md` as a top-level
  contributor document.
- Process: `CONTRIBUTING.md` already delegates maintainer decision rules and
  conflict resolution to `GOVERNANCE.md`.
- Process: `docs/release-checklist.md` is the existing release gate source of
  truth.
