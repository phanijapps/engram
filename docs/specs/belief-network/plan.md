# Plan: Belief Network

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the in-memory adapter with separate belief and contradiction maps and
implement `BeliefRepository`. This gives later synthesis and detection work a
clear persistence boundary without adding model providers or retrieval behavior.

Tempted to build synthesis now; declining because evidence canonicalization and
confidence policy need a separate spec. Tempted to add retrieval support for
beliefs; declining until result explanation and policy are specified. Tempted to
resolve contradictions on write; declining because contradictions are review
records.

## Constraints

- ADR-0003 keeps Rust behavior in crates and infrastructure behind adapters.
- ADR-0004 keeps accepted contracts stable while belief behavior is deferred.
- Beliefs are stances over evidence, not source truth.

## Construction tests

**Integration tests:** in-memory belief repository tests store and inspect belief
and contradiction records.

**Manual verification:** none.

## Design (LLD)

### Data & schema

Beliefs and contradictions use existing domain types and are stored in separate
maps. No schema promotion happens in this slice.

### Interfaces & contracts

`InMemoryMemoryService` implements `BeliefRepository` for deterministic tests.
The core trait currently exposes writes only; read/query ports can be added when
retrieval and review workflows are specified.

### Component / module decomposition

- `state.rs` owns belief and contradiction maps.
- `belief.rs` owns repository write behavior.
- Tests own fixture construction.

### Failure, edge cases & resilience

The repository stores provided records as-is. Validation, synthesis, detection,
resolution, and lifecycle transitions remain future behavior.

## Tasks

### T1: In-memory belief repository stores beliefs and contradictions

**Depends on:** none

**Tests:**
- Store a belief with source evidence and assert it is accepted unchanged.
- Store a contradiction with targets and assert it is accepted unchanged.

**Approach:**
- Add maps to in-memory state.
- Implement `BeliefRepository` in a focused module.

**Done when:** belief repository tests pass.

## Rollout

This ships as in-memory adapter behavior and tests only. No retrieval, synthesis,
or review UI changes ship here.

## Risks

- Read/query ports are not defined yet, so tests inspect returned writes.
- Confidence validation is still domain/future behavior.
- Contradiction resolution workflow remains unspecified.

## Changelog

- 2026-06-29: initial plan for in-memory belief repository baseline.
- 2026-06-29: implemented in-memory belief and contradiction repository writes.
