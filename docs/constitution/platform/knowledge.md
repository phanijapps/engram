# Platform — knowledge

<!-- ASD-STE100 descriptive register. -->

Engram runs in two modes: as a Rust library (embedded in a host application)
and as a standalone MCP server (`mcp/engram-mcp`). Both share the same
SQLite-backed storage. The MCP server binds to stdio by default and serves
the unified tool surface. Storage lives at `~/.engram/` (per-project
subdirectories). Embedding providers: FastEmbed local (default) or HTTP
remote (configurable). Vector mode can be disabled for fully offline
operation. Ontology packs install from a catalogue on first use (network
fetch) or from local fixture files. CI runs on GitHub Actions: two jobs
(contracts + rust + typescript) with 15 gates total.
