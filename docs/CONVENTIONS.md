# Conventions

This file records repository-wide working rules that are already enforced by
hooks, skills, review agents, or `AGENTS.md`. Substantive changes to these
rules go through an RFC before editing this file.

## 1. Source Order

- `AGENTS.md` is the first operational instruction file for coding agents.
- `docs/domain-data-model.md` is the source of truth for portable memory,
  knowledge, belief, hierarchy, policy, provenance, and evaluation semantics
  until an ADR changes that source.
- `contracts/v1/` contains accepted machine-readable contract artifacts.
- `docs/specs/<feature>/spec.md` defines feature acceptance contracts.
- `docs/specs/<feature>/plan.md` defines implementation strategy and gates.

## 2. Workspace Layout

- Storage-neutral Rust crates live under `core/`.
- Replaceable infrastructure crates live under `adapters/`.
- Native language bridges live under `bindings/`.
- TypeScript packages live under `packages/`.
- Active specs live under `docs/specs/`; the root `specs/` directory is
  obsolete.
- Crate package names may remain stable across path migrations. Rename packages
  only through an accepted spec or ADR.

## 3. RFC Lifecycle

Convention, charter, architecture-governance, and public-contract policy changes
start as RFCs under `docs/rfcs/`.

An RFC should name its follow-on artifacts. If it changes this file, its
proposal should include the intended text or a close equivalent. After the RFC is
accepted, the implementation commit should cite the RFC.

Trivial typo, formatting, or broken-link fixes do not need an RFC.

## 4. Spec Metadata Contract

Every spec under `docs/specs/<feature>/spec.md` uses these metadata rules:

- `Status` values are `Draft`, `Implementing`, `Shipped`, or `Deferred`.
- A PR that starts implementing a spec changes `Status` from `Draft` to
  `Implementing` unless the spec ships in the same PR.
- A PR that completes a spec changes `Status` to `Shipped`.
- A shipped spec has every acceptance criterion marked `- [x]` or explicitly
  deferred inline as `(deferred: <anchor>)`.
- Deferred anchors point to headings in `docs/backlog.md`. If a change creates
  the first deferral, it creates that backlog file in the same PR.
- Intra-repo links and referenced contract files touched by the PR resolve.
- A spec that defines or changes a machine-readable contract names that contract
  in its `Contract:` header.

Plans under `docs/specs/<feature>/plan.md` should name task dependencies,
verification commands, and the reason for any substantial approach change in
their changelog.

## 5. Verification Modes

- Use TDD for pure functions, state machines, protocols, and compact
  invariants.
- Use goal-based checks for workspace layout, manifests, generated files,
  scaffolding, and other artifacts best proven by one-line commands.
- Use manual or end-to-end checks for user-invoked surfaces when internal tests
  are not enough to prove the actual workflow.
- Run the narrowest gate that proves the contract, then run broader repository
  gates before handoff when the change crosses crate, package, contract, or docs
  boundaries.

## 6. Risk Triggers

Any one of these routes work to the full work-loop discipline:

- Unfamiliar territory.
- Multi-person work or review.
- Multi-feature work or dependent tasks.
- Compliance, governance, or security boundaries.
- Structural or public-interface changes.
- Destructive or irreversible operations.
- New dependencies.

## 7. Documentation

- Code documentation explains ownership, boundaries, caller obligations, and
  non-obvious invariants. It does not narrate obvious statements.
- Public Rust modules under `core/`, `adapters/`, and `bindings/` start with
  module docs.
- Public TypeScript package entry points use generated contracts rather than
  redefining domain truth.
- Research notes stay under `docs/research/`; durable decisions stay under
  `docs/adr/`; proposals stay under `docs/rfcs/`.
