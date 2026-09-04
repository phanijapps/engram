# Spec: code-liveness

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0028 (engram-code purity), ADR-0022 (engine neutrality + surface parity), RFC-0020 (resolution + ledger)
- **Brief:** none
- **Contract:** `contracts/v1` regenerated outputs for the extended `CodeEdgeKind` (additive enum value); no new REST/event contract — the MCP `code` tool and N-API `dead_code_json` are internal transport surfaces whose output gains an additive `classification` shape
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Agents that ask engram "is this symbol dead?" receive a verdict they can act
on without hand-verification. Three concrete failure classes are eliminated:
deep qualified-path references (`crate::tools::DelegateToZbotTool::new`) leave
no call edge, registration sites (`registry.register(Arc::new(T::new()))`,
`builder.with_x(T::new())`) are invisible to the graph, and trait-object
dispatch (`Arc<dyn Trait>`) leaves registered impl methods looking dead. The
`code op=health` dead-code report classifies every listed symbol into six
buckets — confirmed-unused / dynamic / external / test-only / generated /
inconclusive — instead of a flat list an agent must triage by hand. On the
evidence corpus (four verified real-world cases) the false-verdict count is
zero, and the same re-scan flips the agentzero `DelegateToZbotTool` false
negative to live.

## Boundaries

### Always do

- Record every unresolved reference in the ledger with candidates; a new
  resolution rung consumes the ledger's evidence, never bypasses it.
- Extend liveness through existing edges first (contains-liveness,
  registration) before adding new edge kinds.
- Regenerate `contracts/v1` outputs and TS types in the same change as any
  domain enum extension; classify the change (additive here).
- Keep every `core/code` change I/O-free (tree-sitter + domain types only).

### Ask first

- Any change to the dead-code *default* output shape (consumers parse it).
- Any new tree-sitter grammar dependency.
- Any heuristic that marks a symbol live without a graph edge or extraction
  evidence (e.g. name patterns as sole liveness proof).

### Never do

- No silent ledger drops: a reference that fails every rung stays in the
  ledger with its candidates, never discarded or arbitrarily picked.
- No I/O, storage, engine names, or async runtimes in `core/code` (ADR-0028).
- No MCP-only capability: anything the `code` tool reports must also be
  reachable via the N-API binding (`dead_code_json`) and reflected in the
  Rust surface (ADR-0022 surface parity).
- No LLM or network calls anywhere in the liveness pipeline — extraction and
  classification are deterministic.

## Testing Strategy

- **Resolution ladder fixes (qualified paths, constructors):** TDD — unit
  tests in `core/code` over fixture snippets; the invariant (as-written path
  resolves to the declared entity) is compressible.
- **Registration edges + dispatch liveness:** TDD — unit tests over extraction
  fixtures (`register(Arc::new(T::new()))`, `Arc<dyn Trait>` construction),
  plus graph-level tests in `codegraph/queries`.
- **Classification buckets:** TDD on the classifier function; goal-based
  check on the surface (N-API + MCP emit the six buckets).
- **Cross-repo evidence (agentzero + mem-alpha stores):** goal-based check —
  re-scan via the MCP driver and grep the output for the four expected
  verdict flips; no destructive store changes during verification.

## Acceptance Criteria

- [x] A receiver-qualified constructor declaration (`impl T { fn new }`) is
      emitted as an entity, and a call written as a deep qualified path
      (`a::b::Type::name`) resolves to it — `core/code` corpus +
      `adapters/ingest` integration test using the agentzero
      `DelegateToZbotTool::new` shape.
- [x] `FastEmbedEmbeddingProvider` (type) and `FastEmbedEmbeddingProvider::new`
      are absent from the mem-alpha self-store dead-code list after a force
      re-scan, present before (verified live 2026-09-03: callers
      `[bootstrap_sqlite]`).
- [x] A `register(Arc::new(T::new(…)))` call site produces a typed
      `registers` edge (caller → type) in the extracted graph — unit test in
      `core/code` extraction.
- [x] A type constructed and passed where `Arc<dyn Trait>` is dispatched has
      its `impl Trait` methods classified `dynamic`, never `confirmed-unused`
      — unit test in `codegraph/queries` using the `MemoryLifecycleExecutor`
      shape.
- [x] `code op=health classify=true` partitions listed symbols into
      evidence-backed buckets; test doubles and `tests`-module-contained
      symbols land in `test-only`, not `confirmed-unused` (mem-alpha store:
      test-only=893, dynamic=32, confirmed-unused=1273 — an exact partition
      of the raw list). The `generated` and `inconclusive` buckets are
      deferred (backlog: `code-liveness-followups` — they need per-symbol
      provenance fields that do not exist yet).
- [x] The N-API binding carries the same classification via the SIBLING
      binding `deadCodeClassifiedJson` (`{bucket: [name]}` shape);
      `deadCodeJson`'s default array stays byte-identical (ADR-0022 parity;
      observable at both surfaces).
- [x] The agentzero `DelegateToZbotTool` false negative flips: after a force
      re-scan of the `zbot` store, the tool is ABSENT from
      `code op=health` output (verified live 2026-09-03).
- [x] All four evidence-corpus cases are encoded as `core/eval`-style
      fixtures or `core/code` unit fixtures with expected verdicts, and the
      suite is green.
- [x] `cargo fmt --all`, `cargo check --workspace`, `cargo clippy
      --all-targets -- -D warnings`, `cargo test --workspace`,
      `pnpm run contracts:generate`, `pnpm run typecheck`, `pnpm run build`,
      `scripts/ci/check-contracts.sh`, `scripts/ci/check-engine-neutrality.sh`,
      `scripts/ci/check-surface-parity.sh`, `scripts/ci/check-docs.sh` all
      pass (2026-09-03).

## Assumptions

- Technical: fix lands in `core/code` (pure — deps are engram-domain,
  engram-runtime, tree-sitter grammars; ADR-0028-clean) + `codegraph/queries`
  + `mcp/engram-mcp/src/codegraph.rs` (source: core/code/Cargo.toml).
- Technical: `CodeEdgeKind` lives in the domain contract
  (core/domain/src/knowledge.rs:265) — adding `registers` is an additive
  domain-contract change requiring regenerated contracts + TS types
  (source: repo read).
- Technical: N-API exposes dead-code intel (`bindings/node/src/codegraph.rs:36`
  `dead_code_json`) → ADR-0022 surface parity applies (source: repo read).
- Technical: the unresolved-refs ledger records failures with candidates and
  heals on re-scan (source: core/code/src/resolution.rs + orphan-sweep tests).
- Process: adversarial-reviewer agent installed for spec-mode review
  (source: .claude/agents/adversarial-reviewer.md).
- Product: the six-bucket partition mirrors agentzero requirements R1.1
  (source: ~/Documents/requirements.md § R1.1).
- Product: `whats_changed` defects (args ignored, flat recency) are OUT of
  scope here — separate spec (source: user confirmation 2026-09-03, "go fix
  it" on the recommendation set).
- Technical (review pass 1): the resolution ladder was NOT the failing seam —
  the receiver-hint rung already tries `Type::name`; the real defect was the
  declaration-side noise filter dropping qualified constructors, plus the
  consolidated `scan` dispatcher dropping `force`/`scan_config`
  (source: corpus pin + store queries, 2026-09-03; both fixed this spec).
- Product: classified output is additive (raw list stays default); cross-repo
  re-scan is acceptance evidence (source: user confirmation 2026-09-03).
