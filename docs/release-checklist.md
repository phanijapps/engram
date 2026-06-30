# Release Checklist

Engram is pre-1.0. This checklist defines gates for publishing crates, npm
packages, or release tags.

The manual `Release Verification` workflow runs these gates for a selected ref.
It does not publish crates, npm packages, GitHub Releases, or tags.

## Required Gates

- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check -p engram-store-vector --features fastembed-tests --tests`
- `cargo clippy -p engram-store-vector --features fastembed-tests --tests -- -D warnings`
- `python3 scripts/validate_contracts.py`
- `.codex/hooks/check-contracts.sh`
- `.codex/hooks/check-docs.sh`
- `pnpm run contracts:check-generated`
- `pnpm run typecheck`
- `pnpm run test`
- `pnpm run build`

## Contract Gates

- Public contract changes are classified as compatible or breaking.
- Breaking changes do not modify accepted v1 contracts in place.
- Generated TypeScript contract outputs are reproducible from source.
- Rust domain serialization remains aligned with accepted wire examples.

## Package Gates

- Crates and packages expose narrow facades.
- Generated files are either reproducible or excluded from hand edits.
- Examples and smoke tests document how to run the shipped surface.
- Release notes include contract, Rust, TypeScript, adapter, and migration
  impacts.

## Claims Not Allowed Without Evidence

- Production-ready.
- Secure or audited.
- Faster than another memory layer.
- Horizontally scalable.
- Lossless ingestion for arbitrary code or documents.
- Safe automatic consolidation or pruning.

## Manual Release Notes

Before any tag, include:

- changed public APIs,
- compatible and breaking contract changes,
- new adapters or package entry points,
- known limitations,
- benchmark results if performance is discussed,
- migration steps.
