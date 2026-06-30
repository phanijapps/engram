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
- Keep native bindings as a JSON transport over Rust behavior.
- Keep source-grounded knowledge ingestion deterministic and separate from
  memory records.
- Keep sqlite-vec vector tests available without making embeddings canonical.
- Keep hierarchy navigation scoped and provenance-preserving.
- Keep belief and contradiction records distinct from source truth.
- Keep dry-run consolidation reports auditable before adding mutating sleep
  tasks.
- Keep mutating consolidation behind explicit evaluation gates and injected
  executor ports.
- Keep the in-memory consolidation executor scoped, auditable, and conservative:
  exact duplicate compaction archives later records and records consolidated
  events.
- Keep in-memory decay policy-driven: due active memories expire, legal hold
  wins over expiry, and expired lifecycle events carry the audit trail.
- Keep hierarchy construction incremental: base memory nodes are deterministic
  and auditable before aggregate clustering or retrieval expansion exists.
- Keep hierarchy retrieval expansion opt-in through hierarchical mode and
  bounded to scoped sibling memory base nodes until aggregate construction has
  its own spec.
- Keep aggregate hierarchy construction deterministic and entity-backed until
  semantic clustering and model summaries have quality specs.
- Keep belief synthesis evidence-bound: explicit memory assertions can produce
  beliefs, while inference and semantic contradiction work stay separate.
- Keep explicit assertion contradiction detection review-only: new
  contradictions create open records and source events without mutating source
  truth.
- Keep contradiction resolution explicit and review-record-only until target
  mutation policy has its own spec.
- Keep belief retrieval explicit in result type: derived beliefs can be recalled
  through shared fusion without pretending to be source memories or chunks.
- Keep contradiction-aware belief ranking advisory: open review records reduce
  rank and explain why without hiding or mutating beliefs.
- Keep runtime adapters as wrappers over client transports.
- Keep public repository docs honest about pre-1.0 readiness and release gates.
- Keep filesystem source discovery behind the `SourceReader` port.
- Keep Git worktree discovery tracked-file-only until history semantics are
  specified.
- Keep code-symbol chunking deterministic and declaration-oriented until AST
  parser contracts are specified.
- Keep hybrid retrieval fusion deterministic, traceable, and independent of
  concrete stores or embedding providers.
- Keep in-memory retrieval wired through the fusion port before final context
  truncation.
- Keep in-memory knowledge chunks retrievable as chunks through the same fusion
  path as memory candidates.
- Keep sqlite-vec retrieval exposed through injected query-vector and target
  resolver ports.
- Keep injected retrieval indexes composed through in-memory fusion and
  source-failure reporting without making vector providers canonical.
- Keep FastEmbed BGE-small query-vector generation opt-in behind the vector
  crate feature.
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
