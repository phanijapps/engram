# Security sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.6.

## Charter

Engram's authority, lineage and agent-install trust boundaries: LLM/model SDK exclusion from core/adapters, stub-free shipped code, tamper-evident audit chain, launcher install trust.

## Knowledge — posture and trust boundaries

No LLM/model SDK in core or adapter manifests (deterministic core; LLM calls
stay TS-side — PR-002, gate-enforced). Shipped code carries zero stub//mock markers (baseline 0, measured). No secrets in artifacts;
launcher install is name-tagged (only removes its own files). The L3 audit
chain (events.jsonl) is the tamper-evident record of privileged actions.
Embeddings: FastEmbed local or HTTP provider — never a hard network dep
(disabled-vector mode is fully offline at query time).

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| SEC-101 | MCP operations MUST carry the launch-resolved scope: `org`→tenant and domain/subdomain or project→workspace; unrelated projects/sibling subdomains MUST NOT blend through fused recall or writes. Scope selection MUST NOT be advertised as authentication of a remote user. | refines: PR-005, PR-001; scope/isolation fixtures; host-authentication boundary review. | `mcp/engram-mcp/src/scope.rs:1-5,25-48`; `mcp/engram-mcp/src/app.rs:18-24`; `adapters/integration/src/fixtures/memory.rs:39-45`. |
| SEC-102 | Code register/bind/rebuild/inspect/reset MUST use the shared `CodeBindingAuthorizer`; recipe defaults MUST deny until a host explicitly supplies policy. A synthetic transport actor, repository path or caller branch MUST NOT create authority; `ScopeSetAuthorizer` MUST be described honestly as allowing every actor/repository inside its configured scopes. | refines: PR-005, EN-003; authorizer/transport fixtures, not neutrality regex proof. | `core/integration/src/code_binding/mod.rs:90-107,130-184`; `core/integration/src/code_binding/registration.rs:6-47`; `docs/architecture/domain-data-model.md:61-78`. |
| SEC-103 | Code candidates MUST be constrained by published-generation eligibility before they reach reranking/context composition; unavailable code authority MUST deny dependent code evidence while retaining independently authorized non-code results and reporting the source failure. Exact historical source reads MUST require their authorized lease, not another request's retained generation. | refines: PR-005, SP-001; bound-retrieval/source fixtures. | `core/integration/src/sqlite/recall.rs:163-189,297-375`; `core/integration/src/code_binding/source.rs:715-779`; `docs/architecture/domain-data-model.md:80-85`. |
| SEC-104 | Code reset MUST require preview→owner gate→apply/resume over an immutable hash-bound exact target journal; ambiguous ownership, substituted targets and changed preservation inventories MUST deny rather than expand to a scope wipe. Later lane failure MUST remain reported BestEffort recovery, not invented cross-store rollback. | refines: PR-005, CE-003, PC-001; reset recovery/real-store fixtures; approval authenticity is a separate host obligation. | `docs/architecture/domain-data-model.md:63-67`; `core/integration/src/code_binding/reset.rs:174-227,241-260`; `adapters/integration/tests/code_reset_recovery.rs:240-304,770-852,1216-1250`. |
| SEC-105 | Bound Git acquisition MUST reject traversal/nonportable retained-path identities rather than rename them, enforce independent object/tree/depth/inflation/snapshot limits, and retain explicit cleanup-pending custody on owned cleanup failure; these adapters MUST NOT infer deletion authority for successful referenced generations. | refines: PR-005, PR-004, PC-001; acquisition safety/resource fixtures and focused review. | `docs/architecture/domain-data-model.md:87-96`; `adapters/ingest/src/git_source/safety.rs:25-52,55-87`; `adapters/ingest/src/filesystem.rs:194-219,222-243`. |
| SEC-106 | Agent-bundle install/remove MUST touch only Engram-owned name-tagged entries, protect authored source trees and refuse symlinked destination components; a same-named foreign skill/agent MUST NOT be overwritten or removed. Packaged skills MUST NOT smuggle symlinks into a host's executable agent context. | refines: DD-002, PR-005, PC-001; installer/payload tests; distribution header check alone is insufficient. | `packages/launcher/agent-install.js:48-127,150-180,369-456`; `packages/launcher/scripts/generate-skills-payload.cjs:36-59`; `packages/launcher/scripts/check-skills-parity.cjs:92-95,138-149`. |
| SEC-107 | Recalled constitution text MUST remain background data, not executable approval/instructions. Recall hooks MUST remain bounded, relevance-gated, kill-switchable and fail-open for the host session; that availability policy MUST NOT weaken fail-closed code authorization or allow memory to impersonate the factory owner's approving instruction. | refines: PR-005, CE-003, CI-002; hook/install fixtures and factory approval review. | `packages/launcher/hooks/recall-context.cjs:3-21,239-275`; `core/integration/src/code_binding/mod.rs:130-150`; `.agents/skills/engram-factory/scripts/factory.py:470-484`. |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
