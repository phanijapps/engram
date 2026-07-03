# ADR-0013: Promotion via a caller-invoked validation-event trigger family

- **Status:** Proposed
- **Date:** 2026-07-03
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0011 (consolidation trigger policy — explicit-command baseline this extends), RFC-0007 (federated assertion reconciliation — D4), ADR-0012 (SourceAssertion type — the records this promotes), ADR-0010 (behavior port split)

## Decision summary

- **Decision:** We will promote a claim from `candidate` to trusted through a validation-event trigger *family* (`human-gate | corroboration | time-settling | explicit-command`), config-selected per profile and conditional on scope/source-authority, evaluated only inside a caller-invoked `consolidate()` cycle.
- **Because:** one reconciliation core serves the enterprise gate, a personal coding agent, and an autonomous research agent by swapping the trigger — not by forking the codebase.
- **Applies to:** promotion of `SourceAssertion`/belief fact-state within `engram-consolidation`; not any scheduling/auto-firing runtime.
- **Tradeoff accepted:** promotion runs only when a caller invokes a cycle, so "trusted" can lag the arrival of evidence until the next cycle.
- **Revisit if:** a target domain needs a promotion trigger outside `{human-gate, corroboration, time-settling, explicit-command}`, or a scheduler is required (its own ADR per ADR-0011).

## Context

ADR-0011 set the consolidation trigger policy to **explicit-command**: a cycle runs only when a caller invokes `ConsolidationService::consolidate`, and `engram-consolidation` stays scheduler-free (no timers or background loops). Its `ConsolidationRequest.strategy` records *why* a cycle runs (`Manual`, `TimeWindow`, `EventCount`, `RetrievalFailure`, `Hybrid`).

RFC-0007 (accepted) adds federated `SourceAssertion`s (ADR-0012) that enter as `candidate` and must be promoted to trusted. The three target deployments need different promotion evidence: an enterprise review gate needs human approval; an autonomous news-research agent has no human and must promote on corroboration and time-settling; a personal coding agent promotes on a confirmation, a commit, or a test pass. The open question is how to express "what promotes candidate → trusted" without either (a) forking a promotion mechanism per deployment or (b) violating ADR-0011 by introducing auto-firing triggers.

Constraint: promotion performs durable mutation, so it must stay caller-attributed and auditable; and it must not pull a timer/event runtime into `engram-consolidation` (ADR-0011, ADR-0010).

## Decision

We will express promotion as a **validation-event trigger family** — `human-gate | corroboration(min_sources, min_authority) | time-settling(window) | explicit-command` — selected by configuration per reference profile and applied conditionally on scope and source authority. The family is wired to a gated promotion through the existing `GatedConsolidationService` / `evaluation_gate.rs` path.

Crucially, these are trigger **reasons evaluated inside a caller-invoked `consolidate()` cycle**, never self-firing. "Corroboration" and "time-settling" are *conditions checked when a cycle runs* (does this claim now have N independent sources above the authority bar? has it survived the settling window?), not background schedulers. This extends ADR-0011's `strategy`-as-reason model; it does not reverse its scheduler-free constraint. A future scheduler that invokes `consolidate()` on a policy remains out of scope and would need its own ADR.

Three reference profiles ship as presets of this one policy:

| Profile | Authority | Promotion trigger | Human gate |
| --- | --- | --- | --- |
| `enterprise-gate` | semantic / record / policy | human-gate | on |
| `personal-default` | user-word-wins; code/tests auto-trusted | user-confirmation, commit, test-pass | mostly implicit |
| `autonomous-research` | source trust-score | corroboration + time-settling | off |

Promotion policy is conditional: the human gate is a config that can be enabled/disabled globally and further scoped by source authority (e.g. auto-promote wire-service claims on corroboration while still gating social-media claims).

## Decision drivers

- **One core, many deployments** — the same engine must serve three profiles via config, or Engram fragments into three products.
- **Preserve ADR-0011** — no auto-firing triggers, no scheduler runtime in `engram-consolidation`; promotion stays caller-invoked and auditable.
- **Invisible by default** — for personal/autonomous profiles, promotion must run without a human in the loop; a review workflow is an enterprise add-on, not core.

## Consequences

**Positive:**
- The enterprise gate, personal coding agent, and autonomous research agent run the same promotion engine, differing only by config.
- Every promotion stays caller-attributed and auditable, consistent with ADR-0011.
- No new runtime dependency; reuses `GatedConsolidationService`/`evaluation_gate.rs`.
- The human gate becomes a configurable, scope-conditional switch rather than a hardcoded step.

**Negative:**
- Promotion lags evidence: a claim that gains its Nth corroborating source between cycles is not trusted until the next caller-invoked cycle.
- The trigger family plus per-scope conditioning is more configuration surface to get wrong; profiles must ship sane defaults.
- "Corroboration independence" (counting genuinely independent sources) is a non-trivial sub-problem left to the autonomous profile's implementation.

**Revisit if:** a target domain needs a promotion trigger outside `{human-gate, corroboration, time-settling, explicit-command}`, or an auto-firing scheduler becomes necessary (which is a separate ADR per ADR-0011).

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** `engram-consolidation` gains no timer/scheduler/background-loop dependency; corroboration/time-settling are evaluated only within an invoked `consolidate()` (no code path fires promotion without a caller); each reference profile resolves to a concrete trigger + authority config.
- **Owner:** phanijapps

## Alternatives considered

- **Reuse the existing `Manual`/`Hybrid` strategy as-is** (rejected against *one-core-many-deployments*): the strategy field records a reason but carries no promotion semantics (thresholds, settling window, gate on/off), so each deployment would hand-roll promotion.
- **A dedicated `Gate` strategy tied to an SDLC gate** (rejected against *one-core-many-deployments*): hardcodes the enterprise case and gives the personal/autonomous profiles nothing.
- **An auto-firing scheduler (time/event-driven promotion)** (rejected against *preserve-ADR-0011*): reintroduces surprise mutations and pulls a runtime into `engram-consolidation` that ADR-0011 deliberately kept out.
- **External-orchestrator-only** (rejected against *invisible-by-default*): pushes all promotion logic outside Engram, so the belief layer can't promote without bespoke glue per host.

## References

- ADR-0011 `docs/adr/0011-consolidation-trigger-policy.md` (explicit-command baseline, scheduler-free constraint).
- RFC-0007 `docs/rfcs/0007-federated-assertion-reconciliation.md` (D4, trigger family, reference profiles).
