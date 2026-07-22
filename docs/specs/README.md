# Specs

This directory holds **active** spec-driven implementation slices. Each feature
directory owns a `spec.md` contract and a `plan.md` implementation strategy.

The historical feature specs were consolidated: the capability roll-up + status
lives in [`docs/product/engram.md`](../product/engram.md), open/deferred items in
[`docs/backlog.md`](../backlog.md), durable decisions in [`docs/adr/`](../adr/),
and accepted behavior contracts in [`contracts/v1/`](../../contracts/v1/). The
spec *process* (how to author a new spec) lives in the `new-spec` + `work-loop`
skills and [`docs/CONVENTIONS.md`](../CONVENTIONS.md).

## Active

- [`knowledge-graph-identity`](knowledge-graph-identity/spec.md): storage-neutral,
  caller-policy-driven identity operations for KG entities and exact relationships,
  plus transactional duplicate consolidation. All six RFC-0014 decisions (D1–D6);
  focused `EntityIdentityRepository` port. Constrained by RFC-0014, ADR-0022.
  Draft.
- [`surreal-identity-cell`](surreal-identity-cell/spec.md): the SurrealDB adapter
  cell implementing `EntityIdentityRepository` over embedded SurrealKV with
  SURQL-native semantics (UNIQUE indexes, UPSERT, BEGIN TRANSACTION, MERGE).
  Depends on `knowledge-graph-identity` E0–E1. Constrained by RFC-0014,
  ADR-0022. Draft.
