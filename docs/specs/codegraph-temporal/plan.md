# Plan: codegraph-temporal

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

**Light-mode lean fill.**

## Approach

A new on-top crate `engram-codegraph-temporal` at `codegraph/temporal/` that
scores versioned symbols by recency, blast-radius-weighted impact, and a compound
blend — the first three of memtrace's six temporal modes. Pure math over a
`VersionedSymbol` input (key + `valid_from`/`valid_until` from ADR-0019 + in/out
degree); depends only on `chrono`.

## Tasks

### T1: crate + 3 scoring modes + tests
**Done when:** `cargo test -p engram-codegraph-temporal` green; fmt + clippy clean.

## Changelog

- 2026-07-09: initial plan (light mode); 3 of 6 modes.
