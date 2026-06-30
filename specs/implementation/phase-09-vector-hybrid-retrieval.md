# Phase 09 Spec: Vector And Hybrid Retrieval

## Status

Done for sqlite-vec vector index baseline and opt-in FastEmbed BGE-small smoke
path; hybrid fusion remains future work.

## Scope

Add semantic retrieval as a replaceable secondary index. SQLite vector testing
must use `sqlite-vec` with FastEmbed BGE-small embeddings.

## Acceptance

- Canonical records remain usable without vectors.
- Vector failures are reported as source failures.
- Fusion traces explain hybrid ranking.
