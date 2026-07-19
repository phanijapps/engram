# Accepted behavior — forget-mode contract examples

> Behavior authority for forget modes, migrated from the
> `forget-mode-contract-examples` feature spec. The example payloads + validator
> below are normative.

v1 accepts four `DeleteMode`s. The canonical default is **tombstone**
(`forget-request.json` / `forget-result.json`); mode-specific examples live
alongside under `contracts/v1/examples/`.

| Mode | request `mode` | result `status` | event `kind` | event `payload.mode` |
| --- | --- | --- | --- | --- |
| tombstone (default) | `tombstone` | `tombstoned` | `forgotten` | `tombstone` |
| delete | `delete` | `deleted` | `forgotten` | `delete` |
| redact | `redact` | `redacted` | `redacted` | `redact` |
| archive | `archive` | `archived` | `forgotten` | `archive` |

`redact` is the only mode whose event `kind` is `redacted`; the other three emit
`kind: forgotten`.

**Example files.** `forget-{request,result}.{json,delete,redact,archive}.json`
(8 files), all deserializing as `ForgetRequest` / `ForgetResult`.

**Invariants.** The four modes are not interchangeable outcomes; the mode is
surfaced in `event.payload.mode` (never hidden in event kind alone); physical
deletion is not required for every adapter; examples carry portable payloads only
(no SQL row / in-memory / vector state).

**Enforcement.** `tools/scripts/validate_contracts.py` (accepted-examples map +
a redaction semantic check — redacted memory content must not be returned in a
`ContextPayload`) and `.codex/hooks/check-contracts.sh` (hard-requires the example
files). Execution behavior is exercised at
`adapters/sqlite/tests/service.rs` (`sql_service_forget_delete_removes_memory_and_keeps_event`).
