# Spec: FastEmbed passage embeddings + semantic retrieval (demo Slice 3)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0003, `docs/specs/fastembed-query-provider`, `docs/specs/ci-vector-feature-gates`
- **Contract:** none
- **Shape:** service

## Objective
FastEmbed was query-only and test-only. This slice turns it into a real
end-to-end semantic-retrieval path: a feature-gated `NativeRetrievalEngine`
chunks text, embeds each passage with BGE-small, indexes vectors in sqlite-vec,
and answers queries with BGE-small query embeddings + nearest-neighbor search —
surfaced over the binding and the demo backend, with a frontend search panel.
The model download is runtime-only (the demo build enables the feature; default
builds stay free of model-bearing code).

## Boundaries
### Always do
- Keep FastEmbed behind the `fastembed` cargo feature on `engram-node`; default
  `cargo check -p engram-node` must not compile it.
- Reuse the existing `FastEmbedBgeSmallQueryProvider` (add `embed_passage`) and
  `SqliteVectorIndex` — no new vector infrastructure.
### Ask first
- Adding the `fastembed` feature + `engram-store-vector` dep to `engram-node`
  (pre-authorized by RFC-0003 Slice 3 / user "Full FastEmbed" decision).
### Never do
- Make FastEmbed part of the default workspace build or download models at build
  time (compile ≠ download; downloads stay runtime-only).
- Change v1 contracts or couple adapters.

## Testing Strategy
- **Goal-based / integration:** real-load index+search smoke over the binding
  (BGE-small passage + query embeddings → sqlite-vec → ranked hits). Plus the
  existing `#[ignore]` FastEmbed test.
- **Goal-based:** `cargo clippy -p engram-node --features fastembed -- -D warnings`
  lints the gated code; default `cargo clippy --workspace` stays fastembed-free.

## Acceptance Criteria
- [x] `FastEmbedBgeSmallQueryProvider::embed_passage` vectorizes passages.
- [x] `NativeRetrievalEngine` (`#[cfg(feature="fastembed")]`) indexes chunked text
  and answers semantic queries over sqlite-vec.
- [x] `build:native` builds with `--features fastembed`; `@engram/node` exposes a
  `NativeRetrievalTransport`; backend has `/retrieval/index` + `/retrieval/search`.
- [x] `demo/frontend` SearchPanel indexes a corpus and shows ranked hits.
- [x] Default `cargo clippy/test --workspace`, `--features fastembed` clippy,
  `pnpm typecheck/test`, isolation gate, and contract/docs hooks pass; real-load
  semantic smoke returned ranked hits.

## Assumptions
- Technical: the FastEmbed BGE-small model downloads in this environment (source:
  the `#[ignore]` test passed 2026-06-30, 25s).
- Product: full multi-source fusion via `engram-retrieval`'s composer (memory +
  knowledge + vector) is deferred — this slice delivers semantic retrieval
  directly; fusion is a larger architecture integration (source: RFC-0003 OQ3;
  user confirmation 2026-06-30).
