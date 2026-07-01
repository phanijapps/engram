# Changelog

All notable user-visible changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Maintenance: this file is updated in the same PR that introduces the
> change. CI will warn (configurable: block) when a PR touches code that
> changes user-visible behavior but does not touch this file.
>
> Entries can be drafted from conventional commits: `git log --oneline`
> filtered to `feat:` and `fix:` since the last tag is a starting point,
> not a finished product. Rewrite for users, not contributors. See the
> [Common Changelog guidance](https://common-changelog.org/) — the audience
> is humans who use the software, not humans who wrote it.

## [Unreleased]

### Added

- Lazy query-time embeddings (BGE-small) generated on demand, cached, and
  persisted to a durable sqlite-vec store; per-query warm-up (hit-rate climbs
  across passes).
- Reciprocal-rank fusion (RRF, configurable k + per-source weights) of graph +
  vector retrieval — true hybrid Q&A over the `RetrievalIndex` seam (RFC-0005 /
  ADR-0009).
- Graph `RetrievalIndex` behind the port — knowledge-graph results now fuse with
  vector results; a documented path to Postgres/pgvector/Neo4j backends.
- Tree-sitter AST chunking for 13 languages (Rust, C/C++/C#, TS/JS, Python,
  Java, Kotlin, Apex, Perl, Bash, PHP) with AST call-edge extraction.
- MCP server (index_repo, search, agentic_search, get_job) for any MCP client.
- Friendlier graph explorer: meaningful node labels, source-file paths, repo +
  neighbor context on click, noise-name deprioritization.
- 8-question and 50-question code-intelligence eval suites + a warm-up benchmark
  (`docs/perf/`).
- Reference architecture + charter + roadmap instantiated for engram.

### Changed

- Demo Q&A now grounds answers in RRF-fused (graph + semantic) evidence.
- Graph view defaults to structural kinds (repo/module/class/function); a toggle
  shows all kinds. Dashboard is the first route.

### Deprecated

- (nothing yet)

### Removed

- (nothing yet)

### Fixed

- (nothing yet)

### Security

- (nothing yet)
