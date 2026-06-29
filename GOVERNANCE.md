# Governance

Engram is currently maintainer-led.

## Maintainer Responsibilities

- Protect accepted contracts from incompatible drift.
- Review ADRs for durable technical decisions.
- Keep contribution expectations clear.
- Require tests or specs for behavior changes.
- Handle security and conduct reports promptly.

## Decision Records

Durable decisions live under `docs/adr/`. Contract changes that break accepted
versions require a new ADR and a new versioned contract package.

## Contract Stewardship

The v1 contract is accepted. Compatible additions may be reviewed through pull
requests. Breaking changes require a future `contracts/vN/` package.
