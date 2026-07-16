# Plan: memory-mcp

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

New binary crate `engram-memory-mcp` at `memory/mcp-server/`, mirroring the
codegraph MCP's hand-rolled JSON-RPC-over-stdio pattern. Wraps `EngramProvider`
(via `engram-integration` with the `sqlite` feature). Two tools: `write_memory`
+ `recall`. Opens from a `--storage-path` CLI arg.

## Tasks

### T1: Crate + server skeleton + recall/write tools + integration test
- `memory/mcp-server/Cargo.toml` (deps: engram-domain, engram-runtime, engram-
  memory, engram-integration [sqlite], engram-store-sql, chrono, futures, serde,
  serde_json; dev tempfile).
- `src/main.rs` — EngramProvider::open from CLI arg; JSON-RPC stdio loop
  (copy codegraph MCP pattern); tools/list (write_memory + recall); tools/call
  handlers (parse args → construct request → call provider handle → serialize).
- Integration test: open provider (tempdir) → write_memory → recall → assert.
- Add to workspace + AGENTS.md shape.

**Done when:** `cargo test -p engram-memory-mcp` green + binary compiles.
