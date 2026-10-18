# Architecture

How engram is *currently* organized — the entry point. Kept at this path
because `scripts/ci/check-contracts.sh` asserts its existence.

- **[`architecture/overview.md`](architecture/overview.md)** — the map: layout
  tree, crates/packages/apps, and how the layers relate. Start here.
- **[`architecture/README.md`](architecture/README.md)** — the architecture
  directory index (what is, not why).
- Historical divergence tracking moved to
  [`history/arch_divergence.md`](history/arch_divergence.md) (frozen
  2026-07-19); current drift enforcement lives in
  `scripts/ci/check-engine-neutrality.sh` and
  `tools/research-parity/doc-drift-registry.json`.
