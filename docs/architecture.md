# Architecture

This file is kept as a compatibility entry point for older specs and hooks.

Use the architecture docs under `docs/architecture/` for current work:

- [`docs/architecture/reference.md`](architecture/reference.md) is normative.
  It defines the architecture and clean-design rules new implementation must
  conform to.
- [`docs/architecture/overview.md`](architecture/overview.md) is descriptive.
  It maps the current repository layout and should be updated when directories,
  crates, packages, or major dependencies move.

The research target this implementation is converging on is
[`docs/research/architecture-design-v2.md`](research/architecture-design-v2.md),
with divergence tracked in [`docs/arch_divergence.md`](arch_divergence.md).

In short: Engram is a contract-first, layered memory/knowledge system. Domain
contracts stay portable, deterministic behavior stays in focused Rust core
crates, infrastructure stays behind adapters, TypeScript owns integration
ergonomics, and no crate or package should mix unrelated responsibilities into
a god module.
