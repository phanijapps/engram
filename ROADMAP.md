# Roadmap

The detailed implementation sequence lives in
`docs/implementation-roadmap.md`. Keep this file as the short status view and
use the implementation roadmap as the spec-driven execution loop.

## Current: Spec-Driven Implementation

- Keep v1 schemas, examples, and specs coherent.
- Validate Rust projections against accepted wire contracts.
- Maintain generated-contract drift checks in CI.
- Keep write-memory behavior covered by executable fixtures.
- Keep retrieve-context behavior covered by exact/keyword fixtures.
- Keep forget lifecycle behavior covered by delete/redact/tombstone/archive
  tests.
- Keep evaluation fixtures executable through the Rust fixture runner.
- Keep SQLite SQL behavior covered by write/retrieve/forget/eval service tests.
- Keep file-backed SQLite construction behind the SQL adapter boundary for
  local durable smoke tests.
- Keep native bindings as a JSON transport over Rust behavior.
- Keep source-grounded knowledge ingestion deterministic and separate from
  memory records.
- Keep sqlite-vec vector tests available without making embeddings canonical.
- Keep hierarchy and belief persistence in focused SQLite adapters; mutating
  consolidation algorithms are deferred until they have durable adapter-backed
  specs instead of a process-local catch-all fixture.
- Keep runtime adapters as wrappers over client transports.
- Keep public repository docs honest about pre-1.0 readiness and release gates.
- Keep governance lightweight and contract-first while the project is pre-1.0.
- Keep contributor validation docs aligned with PR, CI, and release gates.
- Keep benchmark smoke paths local and non-claiming until datasets and targets
  are specified.
- Keep filesystem source discovery behind the `SourceReader` port.
- Keep Git worktree discovery tracked-file-only until history semantics are
  specified.
- Keep code-symbol chunking deterministic and declaration-oriented until AST
  parser contracts are specified.
- Keep hybrid retrieval fusion deterministic, traceable, and independent of
  concrete stores or embedding providers.
- Keep SQLite-backed knowledge chunks retrievable as chunks through the same fusion
  path as memory candidates.
- Keep sqlite-vec retrieval exposed through injected query-vector and target
  resolver ports.
- Keep injected retrieval indexes composed through the storage-neutral retrieval
  boundary and source-failure reporting without making vector providers
  canonical.
- Keep FastEmbed BGE-small query-vector generation opt-in behind the vector
  crate feature.
- Keep vector provider feature gates visible in CI without running model
  downloads by default.
- Keep release verification automated separately from package publishing.
- Keep local usage examples checked and close to the crate or package that owns
  each public API.
- Keep concrete adapters outside `engram-core`.

## Next: Post-Roadmap Slices

- Concrete consolidation task algorithms for aggregate hierarchy and semantic
  contradiction detection.
- Semantic hierarchy clustering and model-assisted aggregate summaries.
- Hosted production embedding provider wiring.

## Later

- Hierarchy construction, retrieval expansion, and contradiction resolution over
  real evidence.
- AST-backed symbol extraction and symbol relationship graphs.
- Benchmarks, security review, release automation, and documentation site.
