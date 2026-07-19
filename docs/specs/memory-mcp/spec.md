# Spec: memory-mcp

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** AGENTS.md, the codegraph MCP precedent, ADR-0022
- **Contract:** none — wraps the existing `EngramProvider` facade; no v1 change
- **Shape:** service

## Objective

An MCP server (`engram-memory-mcp`) that exposes engram's memory operations as
agent-callable tools over stdio JSON-RPC 2.0, letting Claude Code / Cursor / any
MCP client use engram as a persistent memory layer. Slice 1 ships two tools:
`write_memory` (persist a fact/observation) and `recall` (unified recall — one
query fans across facts + graph + vector + lexical + beliefs + associative +
community-summary, fused via RRF). Additional tools (forget, put_entity,
put_relationship, list_beliefs, consolidate) are follow-ups.

## Boundaries

### Always do
- Wrap `EngramProvider` via `engram-integration` (not the N-API binding — the
  codegraph MCP precedent + AGENTS.md transport-layer rule).
- Mirror the codegraph MCP's stdio JSON-RPC 2.0 protocol hand-rolled (no MCP SDK
  dep).
- Open the provider from a storage-path CLI arg (`--storage-path`).

### Never do
- Depend on `engram-node` (the N-API binding) — it's a Node transport, not for a
  Rust binary.
- Change any v1 contract.
- Block on stdio (each request is one line in, one line out, flushed).

## Testing Strategy
- **Integration: TDD** — open a provider (tempdir), write a memory, recall it,
  assert the memory appears in the recall results.
- **Build: goal-based** — the binary compiles + the JSON-RPC protocol is correct.

## Acceptance Criteria
- [x] `engram-memory-mcp` at `memory/mcp-server/` compiles as a binary crate.
- [x] Exposes `write_memory` + `recall` MCP tools over stdio JSON-RPC 2.0.
- [x] An integration test writes a memory via the provider + recalls it.
- [x] Zero v1 contract change; all gates green.
