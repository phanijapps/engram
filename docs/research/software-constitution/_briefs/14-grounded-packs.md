# Brief: Ground the domain packs in ENGRAM (gpt-6.1-sol, think hard)

ROLE: The seven domain packs under docs/constitution/{product,architecture,
build,testing,ci,security,release}/ are generic shells — their rules read like
fortune cookies, with zero engram substance. Root rails 01-09 are fine (real
content: engine neutrality, surface parity, pgvector cells...). Your job:
determine what EACH pack must say ABOUT ENGRAM specifically, grounded in the
actual repository. Write ONE file:
docs/research/software-constitution/14-engram-grounded-packs.md

## Analyze the real engram (cwd = /home/videogamer/projects/mem-alpha)
- Product truth: docs/product/engram.md (mission/scope), README.md,
  docs/architecture/overview.md + reference.md (the golden path)
- Real structure: Cargo.toml workspace (35 crates), packages/ (TS),
  mcp/engram-mcp, the capability×engine grid (ADR-0022)
- Real enforcement: tools/ci/ (15 gates), docs/contracts/constitution/v1/
  registries, .github/workflows/ci.yml
- Real product behaviors: memory write/recall/forget, knowledge graph +
  ontology packs, beliefs, code indexing (engram-code), branch-bound indexes,
  the agent bundle (launcher), L3 events
- Real constraints the root rails encode: engine neutrality, surface parity,
  distribution drift, evidence events, components bijection

## Deliver
For EACH of the seven packs (product, architecture, build, testing, ci,
security, release):
1. What engram-specific facts/obligations belong there — evidence-cited
   (file:line where possible). If a pack's engram content is thin, say so and
   recommend merging or minimal real rules rather than filler.
2. 3-8 REAL rules in the house format (ID, statement with BCP 14, typed refs
   to root rules via governs/refines/enforced-by) — each rule must be
   something that would be FALSE for a different codebase, or it is generic
   garbage (the owner's words). Example of the bar: architecture pack should
   carry rules like "recall must compose through the retrieval composer — no
   lane may bypass MemoryService::retrieve" (that's engram-specific).
3. The product pack is the most important: what IS engram as a product —
   contract-first agentic memory: memory + KG + belief + code intelligence,
   Rust core / TS bindings / MCP surface; scope boundaries (what engram is
   NOT: not an LLM host, not a vector DB, not an app server).

## Second question (reason carefully): "engram as an engine should help build
the constitution"
- How can engram's OWN capabilities (code index, knowledge graph, recall,
  distill skill, ontology packs) be used to AUTHOR and MAINTAIN constitutions
   — for this repo and for arbitrary repos engram builds? E.g.: distill the
  code graph into candidate architecture rules; mine drift between rails and
  gates; ontology-pack per domain? Be concrete and honest about what the
  engine can and cannot do today.

## Rules
- Every proposed rule must cite evidence from the repo. No generic advice.
- Under ~350 lines. Date 2026-11-01. Analysis only — write ONLY the 14 file.
