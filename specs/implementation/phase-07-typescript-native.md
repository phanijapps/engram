# Phase 07 Spec: TypeScript Client And Native Binding

## Status

In progress.

## Scope

Expose Rust behavior through native bindings and a typed TypeScript client
without creating a second implementation.

## Acceptance

- Public payload types come from generated contracts.
- Native payloads round-trip through Rust domain types.
- Client tests run write/retrieve/forget fixtures against a local engine.
