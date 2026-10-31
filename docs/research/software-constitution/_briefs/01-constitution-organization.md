# Task brief: Constitution architecture analysis (perspective: repository-governance architecture)

ROLE: Analyze the Software Constitution standard from a repository-governance-architecture
perspective, then design how the engram repo adopts it. You answer the owner's **Q1: How to
organize a Constitution for this repo?**

## Standard sources (fetch to verify; facts below are pre-extracted and verified)
- https://softwareconstitution.com (#what, #rules, #levels anchors), /principles, /adoption
- Canonical spec: https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md

Verified key facts:
- v1.1. A Software Constitution = versioned machine-readable rule set + CI gates enforcing
  rules at merge-time. Required artifacts: `constitution/00-INDEX.md` (master index),
  `constitution/NN-TOPIC.md` (one doc per "rail", numbered, permanent once locked),
  `constitution/DEVIATIONS.md` (time-boxed exceptions; Active section MUST be empty by default),
  `constitution/DECAY-WINDOWS.md` (time-boxed clauses of rules themselves),
  `<spec-root>/implementation/registry.json` (named capabilities → single canonical
  implementation + transport wrappers that must import the SSOT), `<spec-root>/schemas/`
  (JSON Schema 2020-12), gates (CI scripts failing on violation).
- SIX RULE CLASSES (conformance bar: ≥1 rule + ≥1 passing gate per class, every push):
  RULE-1 Zero Competing Systems (single-implementation; registry + drift gate);
  RULE-2 Zero Repo↔Prod Drift (declared == deployed);
  RULE-3 Zero Stubs/Placeholders/Mocks reachable from production (static-analysis gate);
  RULE-4 Self-Governance + Canonical-First (privileged actions emit canonical evidence
  events; event families declared in registry BEFORE code references them; undeclared refs fail CI);
  RULE-5 Zero-Deviation Mandate (non-empty Active deviations = violation itself);
  RULE-6 Coherent Integration / No Orphans (bidirectional registry↔component refs;
  on-disk-but-undeclared = orphan = violation).
- LEVELS: L1 Structural (artifacts exist, schemas validate, 6 classes declared) →
  L2 Gate-enforced (every rule has a blocking CI gate) → L3 Self-Governed (emits RULE-4
  evidence events every CI run; audit chain re-derivable) → L4 Platform-Bijected (generated
  meta-index of every named surface, bidirectional refs + bijection drift gate).
  SCCT conformance test: `npx scct <repo>`.

## The engram repo (read these; cwd = /home/videogamer/projects/mem-alpha)
- AGENTS.md — boundary rules (engine neutrality ADR-0022, surface parity Rust facade↔N-API,
  no god modules, per-crate responsibilities). These are exactly what a constitution rail encodes.
- docs/adr/ — esp. 0003-implementation-stack.md (Accepted) and 0022-* (engine grid, backends).
- scripts/ci/ — existing gates: check-contracts.sh, check-docs.sh, check-engine-neutrality.sh,
  check-surface-parity.sh, check-code-docs.sh, check-bound-code-matrix.py,
  check-research-parity-docs.sh. They predate the constitution but ARE gates.
- Layout: core/ (domain, runtime, memory, knowledge, belief, hierarchy, consolidation,
  reflection, retrieval, procedures, integration, eval, graph-analytics, lang, code),
  adapters/ (per-capability engine cells), backends/ (recipe crates), bindings/node,
  codegraph/, mcp/engram-mcp, packages/ (TS workspace), contracts/, docs/.
- Existing SSOTs to build on: packages/contracts (generated), docs/architecture/domain-data-model.md,
  the capability×engine grid (ADR-0022).

## Deliver (in this order)
1. Concrete file tree: constitution/ rails for engram with real NN-TOPIC names
   (e.g. 01-engine-neutrality.md, 02-surface-parity.md, …), recommended exact paths for
   <spec-root> and <gates-root> given scripts/ci/ and contracts/ already exist, and what
   goes in DEVIATIONS.md vs DECAY-WINDOWS.md on day one.
2. Filled-in example 00-INDEX.md (real engram-specific content, referencing the six classes).
3. ONE fully-worked example rail (engine-neutrality or surface-parity): RFC-2119 rule
   statements, the enforcing gate (map to the existing script), the deviation path.
4. Mapping table: each RULE class → engram-specific instantiation. NOTE: engram is a
   library/SDK + MCP server, not a deployed SaaS — RULE-2 needs honest reinterpretation
   (published npm/crates surfaces, or installed launcher skills/agents vs .agents/skills
   source drift). Address this explicitly.
5. Level roadmap L1→L2→L3 (L4 optional) with honest effort estimates and what SCCT checks
   at each level for this repo.
6. Division of labor: constitution vs AGENTS.md vs docs/adr/ vs docs/specs/ — one paragraph each.

## Rules
- BEFORE writing the artifact, run these and cite actual results (or "index empty for <query>"):
  ./target/release/engram-mcp recall --query "constitution gates" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  ./target/release/engram-mcp recall --query "engine neutrality lint" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  ./target/release/engram-mcp recall --query "ADR-0022 backend recipe" --json --limit 5 --storage /home/videogamer/.engram --project agentzero
  (The parent already saw hit_count 0 on "spec driven development factory templates" — record honestly whatever you see.)
- Write full markdown to:
  /home/videogamer/projects/mem-alpha/docs/research/software-constitution/01-constitution-organization.md
  (write tool, absolute path). Title it, date it 2026-10-31, cite the URLs used.
- Analysis only — do NOT modify any other file; do NOT create constitution/ itself.
- Be engram-specific; generic advice is worthless. Real paths, real rule text.
- Under ~350 lines. Concision over completeness.
