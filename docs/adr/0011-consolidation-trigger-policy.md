# ADR 0011: Consolidation trigger policy — explicit-command baseline

## Status

Accepted

## Context

Consolidation runs auditable cycles over memory and knowledge state via
`ConsolidationService` (`engram-consolidation`). The service is deliberately
scheduler-free: `DryRunConsolidationService` and `GatedConsolidationService`
have no repository, scheduler, model-provider, or background-runtime dependency,
so a cycle runs only when a caller explicitly invokes `consolidate()`.

The v2 research leaves the trigger policy open — "Consolidation trigger policy:
time-based, event-count-based, failure-driven, explicit command, or hybrid"
(`docs/research/synthesis.md:298`) — and `docs/arch_divergence.md` flagged it as
an open decision. The chosen policy needs to be on the record before
production-grade consolidation work grows.

## Decision

Adopt **explicit-command** as the baseline consolidation trigger policy.

- A consolidation cycle begins only when a caller (an agent, a CLI, an admin
  tool, or a future orchestrator) invokes `ConsolidationService::consolidate`
  with a `ConsolidationRequest` whose `strategy` records *why* the cycle runs
  (`TimeWindow`, `EventCount`, `RetrievalFailure`, `Hybrid`, `Manual`).
- The service stays scheduler-free. Automatic triggers — time-based schedulers,
  event-count thresholds, retrieval-failure hooks, or a hybrid supervisor — are
  **deferred** behind a separate runtime/scheduler decision and their own ADR.

## Rationale

- **Auditability.** Consolidation performs durable mutations (compaction, decay,
  belief synthesis, contradiction detection). Explicit triggering keeps every
  cycle caller-attributed and reviewable; an unsupervised scheduler would
  introduce surprise mutations and need its own safety story.
- **No new runtime dependency.** The service's value is its deterministic,
  gated, auditable behavior. Adding a scheduler would pull a timer/event runtime
  into a crate that today has none, crossing the boundary the consolidation
  design preserved (see ADR-0010).
- **Composable, not limiting.** An explicit trigger does not preclude automation:
  a future scheduler can be a thin caller that invokes `consolidate` on a policy,
  leaving the service unchanged.

## Consequences

- The `strategy` field on `ConsolidationRequest` is the trigger *reason*; the
  *policy* (explicit command) is enforced by the absence of any auto-invocation
  path. No contract change.
- `arch_divergence.md` "Consolidation as a formal, separately-owned pipeline"
  moves from 80% to 85%; the remaining gap is additional task algorithms and the
  deferred auto-trigger scheduler.
- When automatic consolidation is needed, open a new ADR that introduces the
  scheduler runtime and its failure/isolation guarantees; do **not** add timers
  or background loops to `engram-consolidation`.
