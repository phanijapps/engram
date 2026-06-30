# Spec: Release Verification Workflow

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** infrastructure

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has a manual GitHub Actions workflow that runs the documented release
verification gates for a selected ref without publishing crates, npm packages,
or tags. The workflow exists to verify release candidates, not to perform a
release.

## Boundaries

### Always do

- Keep the workflow manual through `workflow_dispatch`.
- Run the same Rust, contract, documentation, TypeScript, and vector feature
  compile gates listed in `docs/release-checklist.md`.
- Avoid secrets, publishing tokens, package upload steps, or tag creation.
- Keep the release checklist clear that this is verification automation only.

### Ask first

- Add automatic publishing to crates.io, npm, or GitHub Releases.
- Require signed tags, provenance attestations, or release secrets.
- Add paid services or new CI providers.

### Never do

- Publish artifacts from this workflow.
- Claim release readiness when any gate fails.
- Replace maintainer release notes and compatibility review.
- Run model-download or network-dependent FastEmbed smoke tests by default.

## Testing Strategy

- Goal-based: YAML is checked into `.github/workflows/` and docs hooks pass.
- Regression: the workflow mirrors existing documented gate commands rather
  than inventing new release criteria.

## Acceptance Criteria

- [x] `.github/workflows/release-verify.yml` exists.
- [x] The workflow is manually triggered with `workflow_dispatch`.
- [x] The workflow runs contract, docs, Rust, TypeScript, and vector FastEmbed
  feature compile gates.
- [x] The workflow has no package publishing, tag creation, or secret-dependent
  steps.
- [x] `docs/release-checklist.md` documents the manual verification workflow.

## Assumptions

- Process: `docs/release-checklist.md` lists the release gates.
- Technical: CI already proves the required commands work on Ubuntu runners.
- Technical: FastEmbed feature coverage remains compile-only in default gates.
