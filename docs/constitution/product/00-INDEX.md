# Product sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.1.

## Charter

What engram promises: identity, the four substrates (memory / source-grounded knowledge / derived belief / code intelligence), observable write-recall-forget guarantees, freshness semantics, honest capability status.

## Knowledge — what engram IS

Engram is a contract-first, local-first **agentic memory and knowledge engine**.
Its customer is an agent/host that must remember across sessions, retrieve
grounded context, revise evidence-backed beliefs, and understand code without
confusing observations with truth.

**The four substrates (distinct — never merged):**
1. **Memory** — governed experiential records: write/recall/forget with scope,
   policy, lifecycle events, tombstones (not SQL deletes)
2. **Knowledge** — source/document/chunk-grounded graph: entities, relations,
   taxonomy + ontology packs (FIBO, schema.org, +33); source-bound supersession
   retires stale assertions
3. **Belief** — derived, revisable stances with valid-time ≠ record-time;
   contradictions are reviewable signals, never silent overwrites
4. **Code intelligence** — generation-bound indexes (engram-code tree-sitter):
   symbols, typed edges, blast radius; branch-bound per project

Rust owns all deterministic behavior behind ports; TypeScript (N-API + SDK)
owns ergonomics; one unified MCP server (`mcp/engram-mcp`) is the agent
surface; `packages/launcher` ships the installable agent bundle. Retrieval
fuses all substrates into a bounded, explainable ContextPayload.

**Engram is NOT:** an LLM host, a vector database, an app server, or a hosted
control plane. Vector search is a retrieval mechanism, not the identity.
Extended reference: `docs/product/engram.md` (historical).

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| PRD-101 | Engram's product scope MUST remain governed agent memory, source-grounded knowledge, derived belief and code intelligence, delivered by the Rust engine through SDK/MCP surfaces; vector search MUST remain a retrieval mechanism, not the product identity. Engram MUST NOT become an LLM host or application server/hosted control plane without an explicit scope decision. | refines: PR-002, PR-004, CI-001; manual product/ADR review gap. | `docs/product/engram.md:15-37,241-242`; `README.md:20-31`; `docs/architecture/reference.md:16-26`. |
| PRD-102 | Memory writes MUST produce governed `MemoryRecord` state and auditable lifecycle events; forget MUST preserve the declared delete/redact/tombstone/archive semantics rather than mean an undifferentiated SQL delete. | refines: PR-001, PR-005; enforced-by: check_contracts.py (shape/examples only); runtime lifecycle fixtures. | `core/domain/src/memory.rs:83-92,185-207,219-250`; `core/memory/src/lib.rs:96-105`; `docs/product/engram.md:41-47`. |
| PRD-103 | `store_knowledge`/semantic batches MUST distinguish source-derived memory facts from graph entities/relationships, expose `BestEffort` per-step outcomes and retain succeeded steps after failure. Sourced revisions MUST use stable source identity plus a new exact-material key; an immutable retry MUST NOT be represented as a new extraction or a changed payload under the old key. | refines: PR-001, PR-005, PC-001; batch/supersession fixtures, not a dedicated registered gate. | `core/integration/src/batch.rs:1-30`; `.agents/skills/engram-distill/SKILL.md:76-89,119-127`; `adapters/integration/tests/batch_ingest.rs:252-303,353-405`. |
| PRD-104 | Beliefs MUST remain derived, revisable stances distinct from memory and source truth, with valid time separate from record time; contradiction records MUST remain reviewable signals, not silent truth overwrites. Hosts MUST NOT advertise record-time history or fuzzy belief recall where only valid-time/exact-subject lookup is implemented. | refines: PR-001, CI-001, PC-001; belief fixtures and manual claim review. | `core/domain/src/belief.rs:1-5,68-95`; `core/integration/src/recall.rs:24-38`; `adapters/integration/src/fixtures/belief.rs:22-81`. |
| PRD-105 | Recall MUST return a policy-filtered `ContextPayload` that explains fused provenance, omissions and unavailable sources; a failed lane MUST degrade through `source_failures`, not erase surviving lanes or turn an empty degraded result into a claim that nothing relevant exists. | refines: PR-001, PR-005, PC-001; recall/evaluation fixtures; manual interpretation review. | `core/integration/src/recall.rs:21-38`; `core/integration/src/sqlite/recall.rs:195-278,401-415`; `docs/product/engram.md:106-116`. |
| PRD-106 | Bound-code answers MUST identify the authorized scope/`RepoId`, exact `refs/heads/...` binding and published generation; freshness MUST distinguish indexed OID from explicitly observed ref OID, reporting unknown when unobserved rather than inferring authority from checkout `HEAD`. | refines: PR-001, PR-005, SP-001; bound-source/status and transport fixtures. | `docs/architecture/domain-data-model.md:59-85`; `core/integration/src/code_binding/status.rs:8-57,108-132`; `docs/product/engram.md:73-82`. |
| PRD-107 | A capability MUST NOT be called shipped merely because an adapter exists: it MUST be callable through the provider, N-API/TS and supported MCP surface or carry the root-required explicit exception, and `CapabilityReport` MUST describe the selected recipe's actual support. The launcher bundle's host limitations MUST be disclosed, not sold as uniform host behavior. | refines: SP-001, SP-002, SP-003, CI-002; enforced-by: check_surface_parity.py (accessor names only); transport/install fixtures and review. | `docs/constitution/02-surface-parity.md:8-25`; `core/integration/src/capability.rs:10-15,24-131`; `README.md:64-69`; `mcp/engram-mcp/src/app.rs:1-7`. |
| PRD-108 | Ontology/taxonomy MUST remain first-class governed vocabulary, not opaque metadata or a claim of automatic truth enforcement. Pack installation MUST distinguish full structural vocabulary from the bounded launch slice, expose sliced/available counts, and preserve advisory graph validation unless a separately declared policy makes it blocking. | refines: PR-001, PR-005, PC-001; pack fixtures and manual claims review. | `core/knowledge/src/ontology.rs:7-12,16-38`; `mcp/engram-mcp/src/pack.rs:1-14,58-70`; `core/packs/src/slice.rs:1-23,83-106`. |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
