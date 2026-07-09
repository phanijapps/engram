# Architecture Overview

> The map of this repo. Read this first when exploring. Updated whenever the
> directory layout or major dependencies change. The normative counterpart —
> what new code should *conform to* — is [`reference.md`](reference.md).

## Layout

```
.
├── AGENTS.md             # canonical agent context + boundary rules (CLAUDE.md points here)
├── contracts/            # portable JSON schemas + generated contract outputs (v1/)
├── core/                 # storage-neutral Rust crates (the deterministic brain)
│   ├── domain/           # domain types, invariants, serde, version markers
│   ├── runtime/          # shared errors, result type, clocks, ids, policy gates
│   ├── memory/           # memory service + repository ports
│   ├── knowledge/        # knowledge, graph, ontology, source, ingestion ports
│   ├── retrieval/        # retrieval composition + fusion ports (RRF, weighted)
│   ├── orchestration/    # orchestration facade + compatibility re-exports
│   ├── eval/             # deterministic fixtures + regression harness
│   └── graph-analytics/  # pure graph algorithms (PageRank, betweenness, communities, reachability)
├── adapters/             # replaceable infrastructure crates (behind traits)
│   ├── ingest/           # filesystem/git ingestion + tree-sitter AST chunking
│   ├── memory/sqlite/    # memory records/events + write/retrieve/forget
│   ├── knowledge/sqlite/ # graph/chunk/taxonomy/ontology persistence
│   ├── orchestration/belief-sqlite/ # belief persistence (path migration pending)
│   ├── hierarchy/sqlite/ # hierarchy persistence + navigation repository
│   ├── retrieval/sqlite-vec/      # sqlite-vec vector index + feature-gated FastEmbed
│   ├── retrieval/tantivy-lexical/ # BM25 lexical RetrievalIndex (keyword mode)
│   └── retrieval/cross-encoder-rerank/ # cross-encoder reranker (RerankStrategy::cross_encoder)
├── bindings/node/        # N-API JSON transport (engram-node) — a transport, not a second impl
├── packages/             # TypeScript workspace
│   ├── contracts/        # generated TS types + schemas
│   ├── client/           # ergonomic application SDK
│   ├── node/             # native-binding package (wraps engram-node)
│   ├── adapters/         # JS-side framework/gateway integrations
│   └── eval/             # fixture authoring helpers + CLI wrappers
├── demo/                 # enterprise knowledge-platform demo (RFC-0004)
│   ├── backend/          # Hono backend (ingest, graph, Q&A, benchmark, MCP)
│   └── frontend/         # React + shadcn/ui (dashboard, 3D graph, chat)
├── docs/
│   ├── CHARTER.md        # mission, scope, principles (one page)
│   ├── CONVENTIONS.md    # how we work
│   ├── adr/              # architecture decisions (frozen history)
│   ├── rfcs/             # proposals (governance)
│   ├── specs/            # feature specs and plans
│   ├── architecture/     # this directory — current code structure (for contributors)
│   ├── product/          # current product state (roadmap, changelog)
│   ├── guides/           # user-facing docs (Diátaxis)
│   ├── perf/             # performance benchmarks + eval suites
│   └── research/         # research notes, excerpts, links
├── tools/                # shared repo automation (hooks + scripts) — not shipped
├── .codex/               # Codex skills, agents, and validation hooks
└── .claude/              # Claude skills, agents, commands for AI contributors
```

## Crates, packages, and apps

- `engram-domain` — portable domain types (memory, knowledge, belief, hierarchy,
  policy, provenance, evaluation). Depends on nothing infra. **Look first:**
  `core/domain/src/`.
- `engram-runtime` — `CoreError`/`CoreResult`, clocks, id generation, scope
  matching, policy authorizer traits.
- `engram-retrieval` — the retrieval-composition seam: `RetrievalIndex` →
  `RetrievalFusion` (`ReciprocalRankFusion`, `WeightedRetrievalFusion`) →
  `ContextComposer`. Store-free. **Look first:** `core/retrieval/src/reciprocal.rs`.
- `engram-{memory,knowledge,orchestration,eval}` — service + ports per concern.
- `engram-store-sql` — SQLite memory service and repository adapter for local
  in-memory conformance and file-backed smoke paths.
- `engram-store-knowledge-sqlite` — SQLite graph/chunk store; also implements
  `RetrievalIndex` (`GraphRetrievalIndex`) so KG results fuse with vectors.
- `engram-store-belief-sqlite` — SQLite belief and contradiction repository.
- `engram-store-hierarchy-sqlite` — SQLite hierarchy repository and path
  navigation persistence.
- `engram-store-vector` (sqlite-vec) — vector index + the feature-gated
  `FastEmbedBgeSmallQueryProvider`. **Look first:** `adapters/retrieval/sqlite-vec/src/`.
- `engram-store-lexical` (tantivy) — BM25 lexical `RetrievalIndex` implementing
  the contracted `RetrievalMode::keyword` (identifier-aware tokenizer). **Look
  first:** `adapters/retrieval/tantivy-lexical/src/`.
- `engram-rerank-cross-encoder` — cross-encoder reranker implementing the
  contracted `RerankStrategy::cross_encoder` (injected scorer). **Look first:**
  `adapters/retrieval/cross-encoder-rerank/src/`.
- `engram-graph-analytics` — pure graph algorithms: PageRank, betweenness,
  communities (Louvain local-moving), reachability (`in_degree`, `ancestors`,
  `shortest_path`). std-only. **Look first:** `core/graph-analytics/src/`.
- `engram-node` (`bindings/node`) — N-API bridge; JSON in/out over Rust behavior.
- `@engram/node`, `@engram/contracts`, `@engram/client` (`packages/*`) — TS SDK.
- `demo/backend` — Hono API (ingest, graph, RRF-hybrid Q&A, benchmark, MCP).
- `demo/frontend` — React + shadcn/ui (dashboard, WebGL graph explorer, chat).

## Conventions you'll see across crates/packages

- Every Rust crate is small with one responsibility; crate `lib.rs` files are
  facades (module declarations + narrow re-exports + top-level docs only).
- Errors are typed (`CoreError`); no stringly public contracts.
- Store/vector/embedding/model integrations sit behind traits; tests inject
  deterministic stubs.
- FastEmbed is a compile-time Cargo feature (`--features fastembed`); the demo
  degrades gracefully without it.
- TypeScript packages keep `index.ts` as narrow facades; the native binding is a
  transport, not a second implementation.
- Validation gates in `AGENTS.md` § Validation (`cargo fmt/check`,
  `pnpm contracts:generate/typecheck/test`, `.codex/hooks/*`).

## Where to start

1. Read [`docs/CHARTER.md`](../CHARTER.md) — mission, scope, principles.
2. Read [`reference.md`](reference.md) (normative golden path) then this overview
   (descriptive map).
3. Skim [`docs/product/roadmap.md`](../product/roadmap.md) for current direction.
4. Read [ADR-0009](../adr/0009-retrieval-composition-seam.md) + [RFC-0005](../rfcs/0005-backend-agnostic-retrieval-composition.md)
   for the load-bearing retrieval-composition decision.
5. Pick a recent spec in `docs/specs/` (e.g. `backend-agnostic-retrieval`) and
   read its `spec.md` + `plan.md` alongside the code they produced.
6. Run the demo: index a repo via the dashboard, then `POST /bench/lazy` to see
   the RRF-hybrid Q&A + warm-up curve (`docs/perf/lazy-embeddings.md`).
