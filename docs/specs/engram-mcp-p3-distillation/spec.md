# Spec — engram-mcp P3 Distillation Rigor (RFC-0016)

- **Status:** Implemented (P3 complete 2026-07-29; skill synced to agentzero)
- **Mode:** light (skill content + one verification test; no new public tool)
- **Constrained by:** RFC-0016 D3 (deterministic floor + agentic enrichment), D4, D6
- **Branch:** `feat/engram-mcp-core`

## Objective

Make agent-side distillation **reliably produce cross-layer bridging edges** — the
semantic companion to P0's deterministic `describes` floor. The scan links docs to
code only by exact name match; the `engram-distill` skill adds the links the scan
cannot infer (a concept *Authentication* that explains `auth_middleware`).

## Deliverables

1. **Strengthened `engram-distill` skill** (`mcp/engram-mcp/extensions/engram-distill/SKILL.md`):
   - A required **Bridge** step with a discover-and-link procedure: `scan_repo` →
     `search`/`resolve_entity` to find real code symbols → emit `across`-predicate
     edges (`describes`/`realized_by`/`governs`) from each concept to the code/doc
     artifact it relates to.
   - A **Verify** step (`graph_neighbors`) and a bridge-coverage self-check.
   - Rules tightened: bridging is mandatory; link only to real artifacts; honest
     orphans (a fact) beat fabricated edges.
2. **Verification test**: the agentic semantic-bridge path works end-to-end — put a
   concept + a `describes` relationship to a scanned function whose name differs, then
   `graph_neighbors` confirms the link (distinct from P0's exact-name auto-bridge).

## Boundaries

**Always do** — skill content stays agent-side (no server LLM); bridging uses only
ontology predicates; reuse existing tools (scan_repo/search/store_knowledge/
put_relationship/graph_neighbors).
**Never do** — no new MCP tool; no server-side extraction; no fabricated edges.

## Acceptance Criteria

1. The skill states bridging is **required**, gives a concrete discover→link procedure,
   and a verify + coverage-check step.
2. Test: a concept linked to a differently-named scanned function via a `describes`
   relationship is visible in `graph_neighbors`.
3. Gates: `cargo fmt/check/test -p engram-mcp`, neutrality, docs pass.

## Tasks
- **D1** Rewrite `engram-distill/SKILL.md` (required Bridge + Verify + coverage check). · none.
- **D2** Add the agentic semantic-bridge test in `tools.rs`. · TDD · D1.
- **D3** Gates + commit (canonical skill; note agentzero re-sync). · D1,D2.
