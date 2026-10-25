# engram-distill — E2E recording

A recorded run of `engram-distill` against the real built `engram-mcp` binary
over stdio JSON-RPC 2.0. This is the manual-QA verification for spec AC #10
(exercise the built artifact end-to-end and record the observed result).

- **Date:** 2026-07-29
- **Binary:** `target/debug/engram-mcp` (commit 687d4ab)
- **Launch:** `engram-mcp --storage <tmp>/store.db --project demo` (default
  generic ontology/taxonomy; embedding `none`).
- **Driven by:** piped JSON-RPC `tools/call` lines (no live MCP host; the binary
  is the artifact under test).

## Inputs

1. `index_docs` — Markdown doc:
   ```json
   {"name":"index_docs","arguments":{"content":"# Auth\nThe AuthService issues JWT tokens for the onboarding flow.\n","path":"docs/auth.md"}}
   ```
2. `store_knowledge` — extracted KG:
   ```json
   {"name":"store_knowledge","arguments":{"idempotency_key":"auth-distill-1",
     "facts":[{"content":"AuthService issues JWTs for onboarding"}],
     "entities":[{"name":"AuthService","kind":"api"},{"name":"JWT","kind":"concept"}],
     "relationships":[{"subject":"AuthService","predicate":"realized_by","object":"onboarding"}]}}
   ```
3. `recall` — `{"name":"recall","arguments":{"query":"JWT"}}`.

## Observed outputs

- `index_docs` → `Indexed 1 chunk(s). Batch Complete (guarantee: BestEffort).`
- `store_knowledge` → `Batch Complete (guarantee: BestEffort). 1 fact(s), 2
  entities, 1 relationship(s). [Episode: Skipped, Facts: Succeeded, Entities:
  Succeeded, Relationships: Succeeded, Evidence: Skipped, Embeddings: Skipped]`
- `recall("JWT")` →
  ```
  JWT
  ---
  AuthService
  ---
  AuthService issues JWTs for onboarding
  ---
  # Auth
  The AuthService issues JWT tokens for the onboarding flow.
  ```

## What this proves

The recall result fuses **four** sources in one query — the `JWT` entity, the
`AuthService` entity, the agent-written memory **fact**, and the indexed **doc
chunk** — i.e. fused retrieval across the memory + knowledge + docs lanes within
one project workspace (spec AC #4 fused recall, AC #10 distill write, and the
doc-ingestion path of T10). The bulk write is surfaced as best-effort with
per-step status (AC #8). No LLM ran inside the server; all extraction was the
caller's.
