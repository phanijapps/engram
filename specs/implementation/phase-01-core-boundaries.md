# Phase 01 Spec: Core Crate Boundaries

## Status

Done.

## Scope

Keep `engram-core` as ports and behavior contracts. Keep concrete in-memory
state in `engram-store-memory`.

## Acceptance

- `engram-core` exposes traits and errors only.
- In-memory write and retrieval behavior lives in adapter modules.
- Adapter modules avoid god-module structure.
