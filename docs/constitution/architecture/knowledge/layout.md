# Repo layout

<!-- prerequisite: layers -->

## Layout

```text
.
├── AGENTS.md             # canonical agent context + boundary rules (CLAUDE.md points here)
├── contracts/            # portable JSON schemas + generated contract outputs (v1/)
├── core/                 # storage-neutral Rust crates (the deterministic brain)
│   ├── domain/  runtime/  memory/  knowledge/  belief/  hierarchy/
│   ├── consolidation/  reflection/  retrieval/  procedures/  integration/  eval/
│   ├── code/             # deterministic tree-sitter code behavior (ADR-0028)
│   └── graph-analytics/  # pure graph algorithms (PageRank, betweenness, communities, reachability)
├── adapters/             # replaceable infrastructure crates (behind traits)
│   ├── sqlite/           # engram-store-sqlite — ALL SQLite calls (memory/knowledge/belief/hierarchy/vector)
│   ├── pgvector/         # Postgres/pgvector engine cells
│   ├── embedding/http/   # HTTP embedding provider cell (configurable models)
│   ├── ingest/           # filesystem/git ingestion + tree-sitter AST chunking
│   ├── retrieval/        # sqlite-vec · tantivy-lexical (BM25) · associative-graph (PPR) · community-summary (GraphRAG) · cross-encoder-rerank · mmr-rerank
│   ├── consolidation/    # decay executor (policy-expiry, Ebbinghaus)
│   └── integration/      # backend recipe / conformance composition (SQLite wiring home until backends/ split)
├── backends/             # backend recipe crates (ADR-0022): sqlite + pgvector
├── bindings/node/        # N-API JSON transport (engram-node) — a transport, not a second impl
├── mcp/engram-mcp/       # the ONE unified MCP server (RFC-0015): memory + knowledge + beliefs + hierarchy + code intel
├── codegraph/            # on-top codegraph layer (RFC-0012): code-specific crates on engram
│   ├── queries/  temporal/
├── packages/             # TypeScript workspace (contracts / client / node / adapters / eval)
├── prototype/            # enterprise knowledge-platform demo (RFC-0004): Hono backend + React frontend + MCP
├── engram-cc/           # web-based code-graph visualization workspace (engram-cc spec)
├── docs/                 # README (front door) · adr · architecture · constitution · contracts · guides · history · product · research · rfcs
└── .codex/  .claude/     # validation hooks + skills/agents for AI contributors
```

## Crates, packages, and apps

- **Core** — `engram-{domain,runtime,memory,knowledge,belief,hierarchy,
  consolidation,reflection,retrieval,integration,eval,graph-analytics}`. Each
  is small, one responsibility, facade-style `lib.rs`. **Look first:**
  `core/integration/src/` for the provider facade.
- **Engine cells** — `engram-store-sqlite` (all SQLite, including sqlite-vec +
  the feature-gated `FastEmbedBgeSmallQueryProvider`), `engram-store-surreal`
  (SurrealKV embedded, MTREE vectors).
- **Agnostic adapters** — `engram-store-lexical` (Tantivy BM25),
  `engram-store-associative-graph` (PPR), `engram-store-community-summary`
  (GraphRAG), `engram-rerank-cross-encoder`,
  `engram-consolidation-decay`, `engram-ingest`.
- **Binding + SDK** — `engram-node` (`bindings/node`); `@engram/node`,
  `@engram/contracts`, `@engram/client`, `@engram/adapters`, `@engram/eval`
  (`packages/*`).
- **Codegraph layer** (on top of engram, RFC-0012) —
  `engram-codegraph-queries`, `engram-codegraph-temporal`, and the codegraph MCP
  server. These depend only on `engram-domain` / `engram-graph-analytics`; they
  do not own storage.
- **Demo** — `prototype/backend` (Hono: ingest, graph, RRF-hybrid Q&A,
  benchmark), `prototype/frontend` (React + shadcn/ui: dashboard, WebGL graph,
  chat), `prototype/mcp` (the demo MCP server). `engram-cc/` is a separate
  web-based code-graph visualization workspace.

