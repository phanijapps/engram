# Engram Documentation — the front door

One index for everything readable in this repository. Governance, architecture,
contracts, decisions, and research all start here.

## Governance (constitution)

- **[Constitution](constitution/00-INDEX.md)** — CI-enforced rules, rails 01–08,
  deviations and decay windows. Browsable at [`constitution/`](constitution/) (symlink).
- **[Constitution registries + schemas](contracts/constitution/v1/implementation/registry.json)**
  — machine truth (implementation, gates, events, distributions, components) at
  [`contracts/`](contracts/) (symlink).
- Gates: `tools/ci/check_constitution.py` (blocking) and
  `tools/ci/check_job_artifacts.py` (SDD pipeline).

## Architecture

- **[Architecture overview](architecture/overview.md)** — the map: layout tree,
  crates/packages/apps, how layers relate. Start here.
- [`architecture.md`](architecture.md) — legacy entry pointer (CI-pinned).

## Domain truth and contracts

- **[Domain data model](domain-data-model.md)** — source of truth for domain types
  until Rust domain types become the generated-contract source.
- [`contracts/v1/`](contracts/v1/) — portable generated contracts (JSON schemas,
  generated TypeScript). Regenerate with `pnpm run contracts:generate`.

## Decisions, specs, and proposals

- [ADRs](adr/) — accepted/rejected technical decisions (why).
- [RFCs](rfcs/) — design proposals.
- [Research](research/) — notes, excerpts, links; includes the
  [software-constitution adoption analysis](research/software-constitution/).

## Guides and product

- [Guides](guides/) — how-to, reference, tutorials, explanation.
- [Product](product/) — product briefs (absorbs the retired `specs/` acceptance content). [Charters/conventions](CHARTER.md) ·
  [CONVENTIONS](guides/CONVENTIONS.md) · [release checklist](guides/release-checklist.md).
- [Operator runbooks](guides/operator-bound-code-runbook.md) · [history](history/)

## Housekeeping

Retired content (2026-10-31): about.md, benchmarks.md, database-schema.md,
context/, superpowers/, perf/ and generated HTML artifacts — removed; git
history preserves them. Factory job artifacts live under `.engram-factory/`
(untracked) with receipts under `runs/`.
