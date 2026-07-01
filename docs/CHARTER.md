# Charter

> The foundational document for this project. One page, read whole.
> Modeled on the [CNCF project charter pattern](https://contribute.cncf.io/maintainers/governance/charter/):
> mission, scope, and principles in a single place, kept stable and short.

Changes to this file go through an RFC. The rest of the docs in this repo
are scaffolding around it; this file is the why.

---

## Mission

Engram is a contract-first agentic memory layer: a Rust core that owns
deterministic memory, knowledge-graph, and retrieval behavior, with TypeScript
bindings and an SDK for integration — so AI agents get reliable, structured,
long-lived memory instead of opaque, disposable context windows.

## Scope

What this project does:

- Defines a portable, versioned domain model (memory, knowledge, belief,
  hierarchy, policy, provenance, evaluation) as the contract source of truth.
- Owns deterministic behavior in Rust: storage-neutral service + repository
  ports, fusion, policy gates, validation.
- Ships replaceable infrastructure adapters (SQLite, sqlite-vec, ingest) behind
  traits, plus a Node N-API binding and a TypeScript SDK.
- Proves the layer end-to-end in a demo: ingest polyglot code/docs, build a
  knowledge graph, answer grounded questions, visualize the graph.

What this project does **not** do:

- Host or favor any particular LLM — engram is model-agnostic; LLM calls stay
  in TypeScript behind the pi SDK and never enter the Rust core.
- Ship production backends today — Postgres/pgvector/Neo4j are documented
  deployment targets (RFC-0005), not built adapters.
- Own an embedding model — embeddings live behind FastEmbed (feature-gated) and
  are generated lazily at query time, not eagerly at index time.
- Solve distributed cross-store write consistency — the retrieval-composition
  seam is read-path only.

The "does not" list is at least as important as the "does" list. It's how
we — and AI agents working in the repo — know when a request is out of
bounds. If you find the project being asked to do things that aren't on
either list, that's a signal to refine this section, not to drift.

## Principles

The values that resolve ties when reasonable people disagree. Five to
seven, no more.

1. **Contract first.** The domain model and its JSON contracts are the source of
   truth; Rust types and generated TypeScript conform to them, never the reverse.
   `docs/domain-data-model.md` outranks a convenient implementation.
2. **Rust owns the deterministic core; TypeScript owns ergonomics.** Behavior
   that must be reproducible lives in Rust behind traits; integration glue and
   the LLM client live in TypeScript. Neither re-implements the other.
3. **Small crates, explicit responsibilities.** No god modules or god packages —
   a file mixing construction, validation, state, scoring, and persistence is
   split before handoff. Crate roots are facades; behavior lives in focused
   modules.
4. **Infrastructure lives behind adapters.** Storage, vectors, embeddings, and
   models sit behind traits so tests use deterministic stubs and backends are
   swappable (SQLite today; Postgres/pgvector/Neo4j additive).
5. **Policy is visible on every path.** Scope, retention, and allowed-uses
   checks appear on write, retrieve, ingest, consolidate, and forget — never
   hidden in a generic manager.
6. **Lazy over eager, where measured.** Embeddings generate at query time and
   cache, not at index time — the benchmark, not intuition, justified this.

## What's NOT in this charter

To keep this file from becoming everything-and-the-kitchen-sink:

- **Decision history** lives in [`adr/`](adr/). The charter is what we
  believe; ADRs are the choices we made because of those beliefs.
- **Current product state** lives in [`product/`](product/). The charter
  is direction; product/ is where we are.
- **Current architecture state** lives in [`architecture/`](architecture/).
- **Conventions for how we work** live in [`CONVENTIONS.md`](CONVENTIONS.md).
- **Governance** (roles, decision-making processes, voting) lives in
  [`GOVERNANCE.md`](GOVERNANCE.md) if and when the project is large
  enough to need it. Most small/medium projects don't — a single
  maintainer or small group operating by consensus is fine, and forcing
  governance ceremony on a project that doesn't need it produces theater,
  not clarity.

## When to revise

Revise this charter when:

- The mission has actually changed (rare — usually means a fork).
- The scope has shifted enough that PRs are routinely landing for things
  the current scope doesn't cover.
- A principle has stopped resolving ties — it's being ignored, or it
  contradicts another principle in ways we haven't acknowledged.

Revise via RFC. Editing the charter directly without discussion is the
single fastest way to lose the trust this document is meant to build.
