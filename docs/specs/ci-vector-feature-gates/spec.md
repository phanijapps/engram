# Spec: CI Vector Feature Gates

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0006
- **Brief:** none
- **Contract:** none
- **Shape:** infrastructure

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

GitHub Actions and release documentation enforce the same vector feature gate
used locally: sqlite-vec retrieval tests remain part of the default workspace,
and the opt-in FastEmbed BGE-small provider path must compile without running
model downloads.

## Boundaries

### Always do

- Add CI coverage for the `engram-store-vector` FastEmbed test feature.
- Keep the gate compile-only for provider-backed tests.
- Keep deterministic sqlite-vec tests in the default Rust workspace test path.
- Reflect the gate in release and PR validation checklists.

### Ask first

- Run ignored FastEmbed model-download tests in default CI.
- Add secrets, hosted embedding providers, or network-dependent tests.
- Add a paid CI service or non-GitHub release system.

### Never do

- Make FastEmbed a default dependency for the workspace.
- Require model downloads for every contributor PR.
- Hide vector feature failures behind optional release-only checks.

## Testing Strategy

- TDD: the CI command is the same command used in local validation.
- Regression: existing Rust, TypeScript, contract, and docs CI steps remain.
- Goal-based: release docs and PR templates name the vector feature gate.

## Acceptance Criteria

- [x] CI runs `cargo check -p engram-store-vector --features fastembed-tests --tests`.
- [x] CI runs clippy for the same vector feature test surface.
- [x] Release checklist includes the FastEmbed feature compile gate.
- [x] PR template includes the FastEmbed feature compile gate.
- [x] No model-download or network-dependent test is added to default CI.

## Assumptions

- Technical: FastEmbed BGE-small smoke tests remain ignored and feature-gated.
- Technical: `fastembed-tests` is the existing feature name that enables the
  provider compile path.
