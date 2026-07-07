# Reference architecture

> **Normative.** This is engram's *golden path* — the stack, internal building
> blocks, component stereotypes, and cross-cutting standards that new work
> **conforms to**. A feature's low-level design (in its plan) names which blocks
> it reuses and which standards it follows, and justifies any deviation.
>
> This is the **normative** sibling of `overview.md`. `overview.md` *describes*
> how the code is organized today; `reference.md` *prescribes* how new code
> should be shaped. When they disagree, that gap is either drift to fix or a
> decision to record (ADR). Source of truth for the rules: `AGENTS.md` and the
> ADRs under `docs/adr/`.

## Constraints

- **Technical constraints.** Rust workspace (edition per workspace) + TypeScript
  workspace (pnpm). The Rust core is the deterministic brain; TypeScript owns
  integration ergonomics; the Node N-API binding (`engram-node`) is a JSON
  transport between them. Durable storage is SQLite in **WAL** mode (concurrent
  reads during writes); vectors in **sqlite-vec**; embeddings via **FastEmbed
  BGE-small**, feature-gated and opt-in. The LLM is reached through the **pi SDK**
  from TypeScript only — never from Rust. **Deployment platform:** demo-scale
  local deployment today (Hono backend on `:8787`, Vite React frontend); no
  managed runtime or orchestrator yet — production targeting is a future ADR.
- **Organizational / process constraints.** **Contract-first:** `docs/domain-data-model.md` is the source of truth until Rust domain types are accepted as the generated-contract source. **Boundary rules** in `AGENTS.md` outrank local preference (no god modules; crate roots are facades; store/vector/embedding/model/gateway integrations live in adapters). **ADR-gated:** no runtime manifests or implementation code until `docs/adr/0003-implementation-stack.md` exists; run `.codex/hooks/pre-implementation-check.sh` before implementation work. Compatible vs breaking contract changes are classified explicitly.
- **Constraints you cannot change here.** `engram-domain` must not depend on SQL, vector stores, embedding providers, async runtimes, Node, N-API, or TypeScript tooling. The LLM client stays in TypeScript. Embeddings live behind traits so tests use deterministic stubs. The retrieval-composition seam (RFC-0005 / ADR-0009) is **read-path only** — distributed cross-store write consistency is out of scope.

## Solution strategy

- **Architectural style.** Layered and contract-first: portable domain contracts → Rust deterministic core → replaceable infrastructure adapters → native binding (transport) → TypeScript packages (ergonomics) → demo. Memory, knowledge, belief, hierarchy, policy, provenance, and evaluation concepts are kept distinct unless an ADR merges them. State lives behind repository ports; behavior lives in focused modules named for their responsibility.
- **Key technology decisions.**
  - **Rust for deterministic behavior, TypeScript for integration ergonomics** — each owns what it's best at; neither re-implements the other.
  - **SQLite (WAL) + sqlite-vec** for durable, concurrent-read graph + vector storage — zero-ops, file-portable; the read path is backend-agnostic behind `RetrievalIndex` so Postgres/pgvector/Neo4j are additive later (ADR-0009).
  - **Reciprocal Rank Fusion** (`core/retrieval`) for cross-source hybrid retrieval — score-free, robust across incomparable backends; strength configurable, defaults when absent.
  - **N-API JSON transport** — the binding serializes round-trips into Rust behavior; it is a transport, not a second implementation.
  - **Lazy query-time embeddings** (feature-gated FastEmbed) — indexing stays embedding-free; embeddings amortize across queries and persist to `${ENGRAM_DB}.embeddings.db`.
- **Quality-goal strategy.** *Correctness* via contract-first + typed errors + traits-with-stubs. *Modularity* via small crates with explicit responsibilities and the `AGENTS.md` boundary rules. *Testability* via provider/store traits so deterministic stubs replace network/model calls.

## Building-block view / component catalogue

- **Component stereotypes.**
  - **Domain crate** (`engram-domain`) — portable types, invariants, serde, version markers. Depends on nothing infra.
  - **Runtime crate** (`engram-runtime`) — shared primitives: `CoreError`/`CoreResult`, clocks, id generation, scope matching, policy authorizer traits.
  - **Behavior crate** (`engram-memory`, `engram-knowledge`, `engram-retrieval`) — service + ports for one concern each. `engram-retrieval` owns fusion + composition ports and is **store-free**.
  - **Orchestration facade** (`engram-core`) — compatibility re-export layer above split behavior crates; must not re-become the canonical owner of memory/knowledge ports.
  - **Adapter crate** — replaceable infrastructure behind traits (`adapters/memory/sqlite`, `adapters/knowledge/sqlite`, `adapters/retrieval/sqlite-vec`, `adapters/ingest`).
  - **Binding** (`engram-node`) — N-API JSON transport over Rust behavior.
  - **Package** (`packages/*`) — TypeScript facades; `index.ts` files are narrow public surfaces.
- **Reusable building blocks.**
  - `engram-domain` types + `engram-runtime::{CoreError, CoreResult}` — the error/result surface every crate speaks.
  - **Retrieval-composition seam** (ADR-0009): `RetrievalIndex` (per source) → `RetrievalFusion` (`ReciprocalRankFusion` / `WeightedRetrievalFusion`) → `ContextComposer` → `ContextPayload`. New retrieval sources implement `RetrievalIndex`; fusion stays in `core/retrieval`.
  - The scope/tenant model (`Scope { tenant, workspace, environment, … }`) + `Policy` on write/retrieve/ingest/consolidate/forget paths.
- **Composition rules.** Dependency direction: `domain` ← `runtime` ← `{memory, knowledge, retrieval}` ← `orchestration` ← adapters ← binding ← packages/demo. `engram-domain` depends on nothing infra; `engram-retrieval` calls no stores/providers/policy engines; adapters implement core ports; the binding imports no behavior that duplicates Rust. A file mixing construction + validation + state + orchestration + scoring + persistence + error translation must be split by boundary before handoff.

## Crosscutting concepts / standards

- **Error handling.** Typed `CoreError` (variants: `Adapter`, `InvalidRequest`, …) and `CoreResult<T>` everywhere in Rust; no stringly-typed public error contracts. The binding translates `CoreError` to N-API errors; the demo translates to HTTP. Q&A + retrieval paths fail closed (degrade to a safe baseline) rather than throw.
- **Observability.** Today: console logging in the demo backend; no metrics/tracing pipeline yet — a known gap to record before any production target. Ground truth for a change is the demo's HTTP response + the benchmark output (`/bench`, `/bench/lazy`).
- **Security & data handling.** Scope/tenant isolation on every read/write. Policy checks visible on write, retrieve, ingest, consolidate, and forget paths. **API keys read server-side only** — never sent to the frontend, never logged, redacted from errors. Repo scanning canonicalizes paths and confines reads under root; secret-laden files excluded by name/extension. **LLM output is validated before graph writes**; LLM calls are bounded (~30 s, ~100 KB, input truncated). `.env` is gitignored; only `.env.example` is committed; the pi SDK is pinned and `pnpm audit` kept clean.
- **Configuration & environments.** `.env` for the demo (`ENGRAM_DB`, `ENGRAM_EMBEDDINGS_DB`, `ENGRAM_LLM_{BASE_URL,API_KEY,MODEL}`, `ENGRAM_LAZY_EMBEDDINGS`, `ENGRAM_LAZY_POOL`). Scope selects tenant/workspace/environment. FastEmbed is a **compile-time** Cargo feature (`--features fastembed`) with runtime guards so the demo degrades gracefully without it.
- **Testing standards.** Rust: focused unit tests for invariants + integration tests for vertical flows (`cargo test`); `#[ignore]` for model-dependent tests. TypeScript: type tests + compile checks. Contracts reproducible from source (`pnpm run contracts:generate`); never hand-edited. **Verification tooling** lives in `AGENTS.md` § Validation (`cargo fmt --all`, `cargo check --workspace`, `pnpm run contracts:generate / typecheck / test`, `pnpm run build`, `.codex/hooks/check-contracts.sh`, `.codex/hooks/check-docs.sh`) — the work-loop infra preflight reads these as the canonical gate set.
