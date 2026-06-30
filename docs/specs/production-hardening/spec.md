# Spec: Production Hardening

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** repository hygiene

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram presents itself as an open-source project with accurate public status,
clear contribution and governance paths, and a release checklist that prevents
unsupported compatibility, benchmark, or production claims.

## Boundaries

### Always do

- Keep public docs accurate about pre-1.0 status.
- Document who can make project decisions and how conflicts are resolved.
- Document release gates before publishing crates or npm packages.
- Preserve existing CI validation paths.

### Ask first

- Publish packages, create tags, or declare production readiness.
- Add benchmark claims without benchmark results.
- Add governance roles beyond the current maintainer model.

### Never do

- Claim API stability beyond the current compatibility policy.
- Treat local validation as a security audit.
- Mark release automation complete before CI can reproduce artifacts.

## Testing Strategy

- Documentation review through repository docs hooks where applicable.
- Workspace validation to ensure packaging changes still build.

## Acceptance Criteria

- [x] README status matches the implemented pre-1.0 workspace.
- [x] Governance exists and is linked from contributor docs.
- [x] Release checklist documents required gates and unsupported claims.
- [x] The roadmap distinguishes completed public-hygiene work from future
  benchmark, security, and release automation work.
- [x] Documentation checks ignore untracked developer-local Codex assets while
  still validating tracked repository docs and tracked repository skills.

## Assumptions

- Technical: CI already runs contract, Rust, docs, and TypeScript gates (source:
  `.github/workflows/ci.yml`).
- Process: Engram remains pre-1.0 and should avoid production claims until
  benchmark and release automation work exists.
