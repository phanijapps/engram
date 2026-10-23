# ADR-0031: operator proposal approval boundary

**Status:** accepted · **Date:** 2026-10-22 · **Spec:** `docs/specs/operator-proposals/spec.md`

## Context

Agents operating read-only or with untrusted write posture cannot contribute
durable memories. The existing policy authorizer is a write-time check —
necessary but not sufficient for human-in-the-loop review: several default
bootstrap paths construct a permissive authorizer, so "went through
MemoryService" cannot by itself mean "an operator approved this".

## Decision

> An **operator proposal queue** is a distinct durable domain: inert
> pending-write records (`OperatorProposal`) that are structurally excluded
> from every retrieval lane, promoted only through an approval that routes
> through the existing policy-checked `MemoryService::write_memory` path.

- **Memory-write proposals only** in the first accepted slice; knowledge
  proposals require a separately reviewed policy-checked knowledge route.
- **Approval is orchestration** (claim → write → finalize) over a CAS
  repository (`pending → applying → approved`, `pending → rejected`), with a
  deterministic per-proposal idempotency key. There is no cross-store
  transaction: a committed memory with a failed finalize is reported as the
  visible `applying` state with the memory id — never a silent rejection or
  rollback (same honesty rule as `atomic-batch-ingest`).
- **Operator authority is an injected host concern**
  (`ProposalAuthorizer`); payload actors are attribution evidence, never
  authentication.
- **Surfaces (ADR-0022):** Rust facade + N-API + MCP + TS all carry
  submit/list/approve/reject; `CapabilityReport.proposals` reports truth per
  backend (pgvector reports Unsupported until it ships its own cell).
- **Storage:** `adapters/sqlite/src/proposals` owns only its tables — never
  memory tables, even in single-file deployments (memory–knowledge boundary
  rule applies to the queue too).

## Consequences

- Read-only workers gain a contribution path; operators gain an approval
  queue with receipts (`{memoryId, eventId, idempotencyKey}`).
- Queue retention/expiry is initially operator-managed (no automatic purge);
  promotion receipts are permanent audit anchors on the queue record.
- `engram-proposals` joins the engine-neutrality lint's gated paths.

## Alternatives considered

- **Reuse memory statuses (e.g. `archived` as "pending"):** rejected —
  conflates lifecycle with review, leaks pending content into recall-adjacent
  paths, and cannot carry receipts/decisions.
- **Write-then-review (live memory with a "needs review" flag):** rejected —
  unreviewed content would be recallable, violating the inertness guarantee.
- **Generic workflow/approval engine:** rejected — YAGNI; the queue is one
  typed payload kind with a fixed three-transition state machine.
