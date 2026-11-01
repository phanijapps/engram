# Testing sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.4.

## Charter

What demonstrates engram's promises: deterministic fixtures, per-backend conformance, the fail-closed operation×backend×transport matrix — prove the semantic traps (degraded lanes, supersession keys, belief valid-time).

## Knowledge — how proof works here

Deterministic fixtures (no network, no LLM): acceptance fixtures per contract
(`docs/contracts/v1/acceptance/`), conformance per backend, adapter tests,
factory evals (48) + frame evals (23). The bound-code matrix
(operation×backend×transport) is fail-closed. Behavior batteries: cargo test
--workspace, pnpm test, eval fixtures. CI gates in `tools/ci/` carry unit
tests (`tools/ci/tests/`). TDD spine: fixtures land RED before GREEN
(tasks Phase 1).

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| TST-101 | Retrieval changes MUST replay the accepted positive-recall, forbidden-recall, budget-omission and no-result fixtures; the fixture runner MUST also demonstrate that a deliberately forbidden returned target produces failure, rather than merely snapshotting happy-path content. | refines: PR-001, PR-005, PR-006; fixture target `core/eval/tests/fixture_runner.rs`, intended workspace-test execution. | `core/eval/tests/fixture_runner.rs:9-14,32-76`; `docs/contracts/v1/acceptance/retrieval-fixtures.md:7-35`. |
| TST-102 | Memory changes MUST preserve the write→retrieve→forget vertical, cross-tenant isolation, idempotent-write event behavior and delete-with-retained-event semantics; delete/redact/tombstone/archive acceptance MUST remain mode-specific. | refines: PR-001, PR-005; memory service/conformance fixtures, not just schema validation. | `adapters/integration/src/fixtures/memory.rs:23-50`; `adapters/sqlite/tests/service.rs:123-173,219-252`; `docs/contracts/v1/acceptance/forget-modes.md:7-33`. |
| TST-103 | Unified-recall changes MUST demonstrate multi-lane fusion, one-lane and all-lane degraded success, propagation of inner omissions/failures, and a final output budget applied after fusion rather than losing candidates at the inner facts cutoff. | refines: CI-001, PR-005, PR-006; named recall regression fixtures. | `adapters/integration/tests/recall.rs:423-508,510-662,888-927`; `core/integration/src/sqlite/recall.rs:195-219`. |
| TST-104 | Semantic-batch changes MUST prove that multiple memories under one batch key all land, retries converge, partial failure preserves succeeded writes, and a sourced revision retracts only that source's superseded assertions while shared/foreign/code-attributed records remain protected. | refines: PR-001, PR-005, PC-001; batch/supersession regression fixtures. | `adapters/integration/tests/batch_ingest.rs:252-405`; `adapters/integration/tests/source_supersession.rs:403-470,612-717,885-971,1296-1380`. |
| TST-105 | Bound-code acceptance MUST identify the actual operation × SQLite/pgvector × Rust/native/MCP-stdio/MCP-one-shot cells exercised, with independent receipts; missing required cells MUST NOT count as passes. Publication/reset tests MUST include stale/substituted/unapproved previews, resumable failures and preservation of ordinary knowledge. | refines: SP-004, EN-003, PR-005; job-scoped matrix and real-store fixtures; not fully wired in CI. | `tools/ci/check_bound_code_matrix.py:1-7,31-32,106-117,140-185`; `adapters/integration/tests/code_reset_recovery.rs:240-304,706-852,1216-1250`. |
| TST-106 | Vocabulary-pack changes MUST prove identical cached/offline reinstall, hard rejection of off-pin bytes, full structural writes versus launch-slice reporting, project isolation and deterministic offline widening; an advisory validation finding MUST NOT be mistaken for an install rejection. | refines: PR-001, PR-004, PR-006; MCP pack/converter regression fixtures. | `mcp/engram-mcp/src/pack_tests.rs:98-191,247-329,395-444`; `core/packs/tests/convert.rs:31-141`. |
| TST-107 | Belief temporal acceptance MUST distinguish valid-time lookup before/after a boundary from unsupported record-time-history lookup; a bi-temporal field set or a generic `Supported` label MUST NOT substitute for that behavioral proof. | refines: PR-001, PC-001, PR-006; belief conformance fixture; selected-backend execution still required. | `adapters/integration/src/fixtures/belief.rs:22-81`; `adapters/integration/src/harness.rs:47-64`; `core/domain/src/belief.rs:68-95`. |
| TST-108 | Consolidation tests MUST exercise the production provider wiring with `dry_run` true, absent and explicitly false, proving preview requests do not mutate; the gated-service fixture MUST prove a denied `ConsolidationApplyGate` never invokes its mutation executor. A preview label or isolated planner unit test MUST NOT substitute for these flow checks. | refines: PR-005, PR-006, PC-001; production dry-run and gated-apply fixtures. | `adapters/integration/tests/consolidation_dry_run.rs:1-9,87-95`; `core/consolidation/tests/gated_apply.rs:23-64`; `core/consolidation/src/consolidation/mutating.rs:188-213`. |

## Deviations & decay (domain)

- DW-4 historical hash mismatches advisory (2026-11-28)
- DW-5 SDD frontmatter baseline → gate flips blocking (2026-11-28)
- DW-6 rust doc comments 77 + rustdoc 30 errors, strict at close (2026-11-28)
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
