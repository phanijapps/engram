# Governance

Engram is maintainer-led while the project is pre-1.0. The governance model is
intentionally small until the contributor base and release process are proven.

## Decision Model

The project maintainer owns final decisions for:

- accepted contract changes,
- crate and package boundaries,
- release timing,
- security response,
- contributor access,
- ADR acceptance.

Non-trivial technical changes should start as an ADR, RFC, issue, or spec before
implementation. Contract-breaking changes require an ADR and a new versioned
contract path rather than changing v1 in place.

## Maintainer Responsibilities

Maintainers should:

- keep public docs accurate,
- require validation before merge,
- avoid production or benchmark claims without evidence,
- protect user data, policy, provenance, and deletion semantics,
- document compatibility changes before release,
- respond to security reports through private channels.

## Contributor Path

Contributors can start with issues, docs, fixtures, tests, adapters, and
well-scoped implementation specs. Broader ownership may be granted after a
pattern of high-quality contributions and review judgment.

## Conflict Resolution

Technical disagreements should be resolved from contracts, tests, ADRs, and
measured behavior. If consensus does not emerge, the maintainer decides and
records the durable reasoning in an ADR or issue comment.

## Changes To Governance

Governance changes should be proposed through an RFC or issue before editing
this document.
