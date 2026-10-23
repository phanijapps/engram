# Spec: operator proposal queue

**Status:** accepted (implementation slice 2 of the top-3 enhancements adoption)
**Domain:** `docs/domain-data-model.md` §Operator Proposal Model · **Schema:** `contracts/v1` (OperatorProposal*)

## Problem

Autonomous agents with read-only or untrusted-write postures cannot
contribute durable memories: there was no reviewable pending-write path, and
the existing policy gates are write-time checks, not an operator approval
workflow.

## Accepted design

1. **Domain + port + cell.** `engram-domain` owns the queue contract;
   `core/proposals` owns the `ProposalRepository` CAS port, the
   `ProposalAuthorizer` host port, and the `ProposalService` orchestration;
   `adapters/sqlite/src/proposals` is the storage cell (own tables only).
2. **Memory-write proposals only** (typed `ProposalMemoryWrite`). Knowledge
   proposals wait for a separately reviewed policy-checked knowledge route.
3. **Approval = claim → `MemoryService::write_memory` → finalize**, with a
   deterministic per-proposal idempotency key. Policy denial leaves NO
   active memory (tested). No cross-store transaction; `applying` is the
   explicit in-progress/recovery state and rejection can never overtake it.
4. **Structural exclusion from recall:** no proposal handle is reachable
   from any retrieval lane; queue listing is a review operation. The
   lifecycle tests prove the memory store stays empty while
   pending/rejected/denied.
5. **Operator authority is injected** (`ProposalAuthorizer`); the local
   `AllowAllProposalAuthorizer` exists for explicitly operator-enabled local
   configs and tests. Payload actors are evidence, never authority.
6. **Full surface parity (ADR-0022):** Rust facade (`require_proposals` +
   builder + `CapabilityReport.proposals`), N-API
   (`requireProposalsApi` + submit/list/approve/reject JSON), MCP
   (`proposal_submit/list/approve/reject` in `tool_names.txt`), TS
   (`NativeProposalsApiBinding` + provider methods). Parity + neutrality
   lints cover the new crate.

## Non-goals

Auto-approval, agent self-elevation, knowledge/belief proposal kinds, a
workflow UI, a global identity system, queue export/import (explicitly
unsupported initially).

## Acceptance criteria (verified)

- [x] Approve promotes exactly one policy-checked memory; retry is
      idempotent (same memory id, no second record).
- [x] Denying memory authorizer → PolicyDenied error, zero active memories,
      proposal stays `applying`; rejection of an in-flight approval is a
      Conflict.
- [x] Rejected payloads stay inert forever; approving a rejected proposal
      conflicts.
- [x] Authorizer gates submit and approve (deny-all leaves no queue record).
- [x] Repository CAS: one claimant wins; version mismatch conflicts; foreign
      scopes read as absent.
