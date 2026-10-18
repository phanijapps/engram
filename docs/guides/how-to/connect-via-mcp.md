# Connect to engram via MCP

> **⚠ SUPERSEDED — read [`mcp/engram-mcp/HOW-TO.md`](../../../mcp/engram-mcp/HOW-TO.md)
> instead.** Engram ships **one unified server, `engram-mcp`** (RFC-0015),
> covering memory + knowledge graph + beliefs + hierarchy + code intelligence.
> The former two-server model (`engram-memory-mcp` + `engram-codegraph-mcp`) is
> **retired — both crates are deleted**.

The current connection instructions live in one place:

- **[`mcp/engram-mcp/HOW-TO.md`](../../../mcp/engram-mcp/HOW-TO.md)** — build,
  launch flags (`--storage`, `--project`, `--ontology`, `--taxonomy`,
  `--tools core|full|all|consolidated`, `--backend sqlite|pgvector`), and the
  `.mcp.json` client registration for any MCP host (Claude Desktop, Cursor,
  Codex, pi, …).

Quick reference for the default surface: the flagless launch registers the
**21-tool default set** (discovery: `recall`, `search`, `get_context`,
`explore`; code intel: `symbol_context`, `change_impact`, `architecture`,
`whats_changed`; ingest/write: `scan_repo`, `index_docs`,
`store_knowledge`, `put_entity`, `put_relationship`, `write_memory`;
decisions: `belief_get`, `contradiction_list`, `procedure_put`,
`procedure_list`; correct/monitor: `forget`, `graph_health`; content
stitching: `fetch_content`).

The retired two-server instructions were removed from this guide on
2026-10-17; see git history or `docs/rfcs/0015-unified-engram-mcp.md` for
the design record.
