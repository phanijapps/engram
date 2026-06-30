# Specs

This directory holds active spec-driven implementation slices. Each feature
directory owns a `spec.md` contract and a `plan.md` implementation strategy.

## Active

- [`retrieval-composition-boundary`](retrieval-composition-boundary/spec.md):
  moves multi-source retrieval composition out of store adapters and into a
  storage-neutral retrieval boundary.

## Existing Slices

Older slice directories in this folder remain the historical implementation
ledger. Prefer adding new active work to the list above when a fresh spec is
opened.
