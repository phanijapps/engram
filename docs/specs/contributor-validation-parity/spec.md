# Spec: Contributor Validation Parity

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** governance

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Contributor-facing setup and validation docs list the same local gates required
by pull requests and release readiness, including Rust clippy, generated
contract drift, TypeScript build, and the opt-in vector FastEmbed feature
compile checks.

## Boundaries

### Always do

- Keep README and CONTRIBUTING validation commands aligned with PR/release
  expectations.
- Keep commands copy-pasteable for local contributors.
- Keep provider-backed FastEmbed coverage compile-only by default.

### Ask first

- Add new validation tools, package managers, or CI services.
- Change release requirements beyond documentation parity.
- Require model downloads or ignored tests in normal contributor setup.

### Never do

- Change contracts, schemas, generated TypeScript, or runtime code.
- Remove existing contract or documentation hooks from contributor guidance.
- Claim validation proves production readiness or security audit status.

## Testing Strategy

- Goal-based: documentation hooks and diff checks prove the docs are coherent
  and syntactically clean.
- Regression: release and PR checklists remain the source of truth for required
  gates.

## Acceptance Criteria

- [x] README validation includes generated contract drift, Rust clippy, vector
  FastEmbed feature compile checks, TypeScript tests, and build.
- [x] CONTRIBUTING development setup includes the same contributor gate list.
- [x] No runtime, schema, generated contract, or CI changes are introduced.
- [x] Documentation checks pass.

## Assumptions

- Process: `.github/pull_request_template.md` lists the expected PR validation
  commands.
- Process: `docs/release-checklist.md` lists the release validation gates.
- Technical: FastEmbed feature checks compile provider-backed tests without
  running model downloads.
