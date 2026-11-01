# Architecture sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.2.

## Charter

Where engram's semantics live: protect the real seams — composer fusion, adapter cells, registries — not a generic layered diagram.

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| ARC-101 | Portable domain types/invariants MUST stay in `engram-domain`, conforming to the accepted model/contracts; memory, knowledge/ontology, belief, hierarchy, retrieval and consolidation MUST keep separate behavioral owners. New work MUST NOT recreate the retired `engram-core` facade as a canonical port owner or persist knowledge state inside the memory repository for convenience. | refines: CI-001, CI-004, PR-003; enforced-by: check_implementation_registry.py (declared ownership/imports only); boundary review gap. | `docs/contracts/constitution/v1/implementation/registry.json:13-46`; `Cargo.toml:3-19`; `docs/architecture/reference.md:30,44-53`; `core/memory/src/lib.rs:19-35`. |
| ARC-102 | Engine operations MUST remain behind core ports in declared adapter/recipe zones. SQLite's existing `core/integration/src/sqlite/` zone MUST be identified as exempt composition, not evidence that neutral facade files may acquire SQL; a new engine MUST compose the same ports, not fork domain behavior. | refines: EN-001, EN-003, PR-004; enforced-by: check_engine_neutrality.py (listed paths/patterns only); recipe conformance/review. | `docs/adr/0022-engine-grid-vs-backend-recipe.md:8-29,89-111`; `core/integration/Cargo.toml:30-38`; `backends/sqlite/src/lib.rs:7-13`; `backends/pgvector/src/lib.rs:29-35`. |
| ARC-103 | Unified recall MUST obtain the facts lane through `MemoryService::retrieve`, other retrieval candidates through `RetrievalIndex` and beliefs through their repository, then reuse `ReciprocalRankFusion` and `compose_context`; no new lane or transport may substitute its own ranking/composer or preempt the final shared budget. | refines: CI-001, CI-002, PR-005; recall fixtures and focused review, not proven by neutrality. | `core/integration/src/recall.rs:21-30`; `core/integration/src/sqlite/recall.rs:195-219,249-278,401-415`; `docs/architecture/reference.md:49-53`. |
| ARC-104 | `engram-code` MUST own deterministic symbol identity, typed code edges and reference resolution without filesystem/git/storage/LLM behavior; `adapters/ingest` MUST own acquisition/persistence, and `engram-lang` additions MUST register grammars/kind maps rather than fork the chunker. Unresolved references MUST remain visible rather than become invented call edges. | refines: CI-001, PR-002, PR-004; code/language fixtures and dependency review gap. | `core/code/src/lib.rs:1-8,19-36`; `core/lang/src/lib.rs:1-20`; `docs/product/engram.md:54-69`. |
| ARC-105 | Graph algorithms MUST remain dependency-free in `engram-graph-analytics`; code-specific dead-code/blast-radius/dependency-path behavior MUST stay in the on-top `codegraph/` crates, consuming Engram contracts/algorithms rather than owning another store or MCP server. | refines: CI-001, CI-002, PR-003; manifest/ownership review; inventory gate is only partial proof. | `core/graph-analytics/Cargo.toml:1-6`; `codegraph/queries/Cargo.toml:8-12`; `docs/contracts/constitution/v1/implementation/registry.json:71-74,89-99`; `Cargo.toml:32-36`. |
| ARC-106 | N-API, TS packages and unified MCP handlers MUST delegate to canonical Rust ports/provider behavior; JSON translation, host configuration and optional agent-side extraction MUST NOT become a second memory/retrieval implementation or a Rust-hosted LLM client. | refines: CI-002, PR-002, SP-001; enforced-by: check_implementation_registry.py, check_production_completeness.py (imports/dependency patterns only), check_docs.py (TS delegation subcheck). | `docs/contracts/constitution/v1/implementation/registry.json:61-68`; `mcp/engram-mcp/src/app.rs:1-24`; `.agents/skills/engram-distill/SKILL.md:8-18`; `tools/ci/check_docs.py:57-65`. |
| ARC-107 | Ontology-pack conversion/slicing MUST be deterministic bytes-to-domain-record behavior in `core/packs`; fetch/cache/pin verification and launch-config I/O MUST remain in the MCP host path, while durable writes reuse provider ontology/taxonomy ports. Constitution packs MUST NOT be confused with these runtime vocabulary packs. | refines: CI-001, PR-002, PR-004; pack conversion/install fixtures; manual boundary review. | `core/packs/src/converter.rs:1-9,40-60`; `core/packs/src/slice.rs:1-12`; `mcp/engram-mcp/src/pack.rs:1-14`. |
| ARC-108 | Consolidation MUST remain an explicit request→plan/run operation through injected mutation executors, not an incidental write side effect; only `dry_run == Some(false)` may apply. Gated mutation MUST keep pre/post evaluation and apply authorization visible and return task-level audit outcomes; the default allow-all apply gate MUST NOT be presented as host policy enforcement or a post-run regression as automatic rollback. | refines: CI-001, PR-005, PC-001; consolidation planning/apply and production-wiring fixtures; recipe policy/evaluation coverage must be inspected separately. | `core/consolidation/src/consolidation/mutating.rs:56-105,109-150,165-267`; `adapters/integration/tests/consolidation_dry_run.rs:1-9,87-95`; `docs/product/engram.md:118-125`. |

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
