# Spec — Procedures (Layer 6, RFC-0016)

- **Status:** Foundation (domain + port landed); adapter/wiring/proxy/tools pending.
- **Mode:** full (new capability across the stack)
- **Constrained by:** RFC-0016 D5 (procedures were the one deferred layer — no provider handle existed), ADR-0022 (surface parity: a `require_procedures` handle needs a matching `NativeProceduresApi` proxy or the parity lint fails)

## Objective

Add **procedures** — replayable runbooks with success/failure accounting (Zbot's
Layer 6) — as a first-class engram capability: a domain type, a storage-neutral
port, a SQLite adapter, a provider handle, a capability-report key, and an N-API
proxy. This closes the one true *build* gap for full Zbot-class parity.

## Domain model

```text
Procedure { id, scope, name, steps: Vec<String>, trigger: Option<String>,
            success_count, failure_count, provenance, policy,
            created_at, updated_at?, metadata? }
ProcedureStats { total, total_success, total_failure }
```

## Port (`ProcedureRepository`)

`upsert_procedure`, `get_procedure(id, scope)`, `get_procedure_by_name(name, scope)`,
`list_procedures(scope)`, `increment_success(id, scope)`, `increment_failure(id, scope)`,
`procedure_stats(scope)`, `delete_procedure(id, scope)`.

(Mirrors Zbot's `ProcedureStore` minus embedding similarity search, which rides the
existing vector lane when needed.)

## Boundaries

**Always do** — storage-neutral port in a new `core/procedures` crate (mirrors
`core/belief`); SQLite adapter stores JSON + scope index (mirrors `SqlBeliefStore`);
route through the provider; reflect in `CapabilityReport`; add the N-API proxy so the
parity lint stays green.
**Never do** — no LLM inside the server; no embedding/model deps in the port crate;
no coupling to memory/knowledge internals; success/failure accounting is the caller's
to drive (the server records, doesn't decide).

## Acceptance Criteria

1. `Procedure` + `ProcedureStats` domain types; `ProcedureRepository` port in
   `engram-procedures`.
2. `SqlProcedureStore` persists + reads procedures (in-memory + file); success/failure
   counters increment.
3. `require_procedures()` on `EngramProvider`; `procedures` key in `CapabilityReport`;
   wired in the SQLite bootstrap (single-file path).
4. `NativeProceduresApi` N-API proxy → `check-surface-parity.sh` stays green.
5. (Optional, 3rd surface) MCP `procedure_put` / `procedure_list` tools.
6. Gates: fmt/check/test, neutrality, surface-parity, docs.

## Tasks
- **P1** Domain type `Procedure`/`ProcedureStats` (`core/domain/src/procedures.rs`). · done (foundation)
- **P2** `engram-procedures` port crate + `ProcedureRepository` trait. · done (foundation)
- **P3** `SqlProcedureStore` adapter (`adapters/sqlite/src/procedures/`). · pending
- **P4** Provider `require_procedures` + bootstrap wiring + `CapabilityReport` key. · pending
- **P5** `NativeProceduresApi` N-API proxy (parity). · pending
- **P6** (optional) MCP `procedure_*` tools. · pending
- **P7** Gates + commit. · pending
