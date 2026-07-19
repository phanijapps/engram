# Spec: Documentation overhaul — functional README + architecture + guides

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** docs/CONVENTIONS.md (doc standards)
- **Brief:** none
- **Contract:** none — documentation only
- **Shape:** mixed <!-- docs + structure across multiple deliverables -->

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it.

## Objective

Engram's documentation serves two audiences: **techno-functional** readers (80%
— engineers, architects, data scientists who understand tech + need functional
context) and **functional** readers (20% — BAs, product owners, AI strategists
who need to understand what engram DOES without reading Rust). Today the
README is code-heavy and the docs/ tree lacks a synthesized architecture
overview, a storage-extension guide, a build guide, and MCP documentation.

This spec delivers a documentation set where every entry point is accessible to
both audiences: a **functional README** (concepts-first, use-case-driven, less
code), a **Memora-style architecture overview** (pipeline flow diagram with
image placeholders, touching the synthesized research), a **storage-extension
guide** (how to add `engram-store-<engine>`), a **build guide**, an **MCP
guide**, and a **synthesized research index** with backlinks. The README
backlinks to all of these.

## Boundaries

### Always do
- Write for the techno-functional reader first (80%); provide enough functional
  framing for the 20% non-engineer to understand what engram DOES.
- Use image placeholders (`![description](path/to/image.png)`) for all
  diagrams — the user fills in actual images later.
- Backlink from the README to every new doc; backlink from each doc back to the
  README + relevant research/ADR.
- Keep docs in their established home — `docs/guides/how-to/` for how-tos,
  `docs/architecture/` for architecture, `docs/research/` for the synthesis —
  following the existing `docs/guides/how-to/*` precedent (no Diátaxis rule is
  recorded in `docs/CONVENTIONS.md`; the precedent is the source of truth).
- **One canonical home per topic.** A guide owns its topic; an overlapping
  existing README section becomes a one-paragraph summary + backlink (or is
  deleted). The per-section disposition `{keep | thin-to-backlink | delete}` is
  decided in `plan.md` T6 — never carry the same content in two places.
- Present tense, as-built — describe what exists, not what "will be."

### Ask first
- Adding new top-level doc sections that change the docs/ CONVENTIONS structure.
- Any claim about performance, accuracy, or competitive positioning without a
  citation.

### Never do
- Remove the existing README's technical sections (Quick Start, Contracts,
  Development Workflow, Contributing) — they stay; the functional framing is
  added on top. [structural]
- Duplicate content across docs (one canonical home per doc). [structural]
- Edit the domain-data-model.md or ADRs — these are sources of truth. [structural]

## Testing Strategy
- **Goal-based check:** every new doc file exists at its canonical path, the
  README links to it, and it links back to the README.
- **Goal-based check:** manual grep across `README.md` + every new doc file
  confirms no broken internal links (recorded in the PR body — there is no
  `markdown-link-check` config in the repo, so the gate is grep, not a tool).
- **Manual QA:** a recorded **walkthrough checklist** (in the PR body) lists
  each entry point → link navigated, so a reviewer can verify a techno-functional
  reader reaches any guide from the README → architecture → guides without a dead
  end or a code dump.

## Acceptance Criteria
- [x] **Functional README** rewritten for techno-functional audience: concepts
  first, use cases, less code, backlinks to all docs below. Every existing
  section that overlaps a new guide is disposed `{keep | thin-to-backlink |
  delete}` per `plan.md` T6 — no content duplicated across the README and a guide.
- [x] **Architecture overview** (`docs/architecture/overview.md` updated): a
  Memora-style pipeline flow diagram — **image placeholder with an ASCII fallback
  so the doc renders today** — showing data flow from agent input through
  engram's processing to retrieval output; touches the synthesized research
  concepts (memory lifecycle, knowledge graph, belief, hierarchy, retrieval
  composition, consolidation).
- [x] **Core layer docs** — each layer (core libs, adapters, integration facade,
  N-API binding) documented with its responsibility + key types + how it fits
  the pipeline.
- [x] **Storage extension guide** (`docs/guides/how-to/extend-storage.md`):
  how to add a new `engram-store-<engine>` backend, step by step, using the
  SQLite/SurrealDB crates as templates.
- [x] **Build guide** (`docs/guides/how-to/build-and-run.md`): prerequisites,
  build commands (SQLite/SurrealDB features), test commands, demo setup, MCP
  server startup.
- [x] **MCP guide** (`docs/guides/how-to/connect-via-mcp.md`): both MCP servers
  (memory + codegraph), their tools, how to connect (stdio + HTTP), client
  configs (Copilot, Claude Desktop, Cursor).
- [x] **Synthesized research index** (`docs/research/README.md` rewritten):
  synthesized summary with backlinks to each research file + the concepts they
  inform.
- [x] All new docs linked from the README; all new docs link back to the README.

## Assumptions

- **Product:** audience is 80% techno-functional (understand tech + need
  functional context), 20% functional (BAs, data scientists, POs, AI
  strategists). Write for the 80% first; carry the 20% with concept framing.
  (user confirmation 2026-07-16)
- **Technical:** the architecture diagram is modeled on Microsoft Research's
  Memora Figure 1 — a pipeline flow (input → segmentation → memory entries +
  implicit graph → policy-guided retrieval → output), not a formal C4 container
  diagram. Image placeholders + ASCII fallback. (web fetch of the Memora blog,
  2026-07-16: https://www.microsoft.com/en-us/research/blog/memora-a-harmonic-memory-representation-balancing-abstraction-and-specificity/)
- **Process:** docs live in their established home — how-tos under
  `docs/guides/how-to/`, architecture under `docs/architecture/`, research under
  `docs/research/` — following the existing `docs/guides/how-to/*` precedent
  (no Diátaxis rule is recorded in `docs/CONVENTIONS.md`). (verified via `find docs/`,
  2026-07-16)
- **Technical:** reconcile, don't duplicate — `docs/architecture/overview.md`,
  `docs/guides/how-to/build-a-surrealdb-store.md`, and `docs/research/README.md`
  already exist and are updated (not re-created). (verified via `find docs/`,
  2026-07-16)
