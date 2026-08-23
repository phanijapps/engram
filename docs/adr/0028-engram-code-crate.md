# ADR-0028: `engram-code` — a dedicated code-indexing behavior crate

- **Status:** Accepted
- **Date:** 2026-08-23
- **Decision-makers:** phanijapps
- **Related:** RFC-0020 (unified knowledge indexing — Phase 2 vehicle), RFC-0012 (codegraph on-top layer), ADR-0022 (engine neutrality + surface parity), ADR-0017 (multi-repo scope model), `docs/specs/engram-code/` (the feature spec), ADR-0003 (implementation stack)

## Decision summary

- **Decision:** code indexing moves out of `adapters/ingest` into a dedicated behavior crate, `engram-code`, at `core/code`; `adapters/ingest` consumes it to build the code graph.
- **Because:** code extraction has grown past an adapter concern — it owns deterministic domain behavior (symbol identity, typed edges, cross-file resolution, framework patterns) that belongs with the behavior crates, while the adapter keeps only I/O orchestration (filesystem walk, git, hashing, retraction sequencing).
- **Applies to:** the tree-sitter chunker, code-symbol extraction, the noise filter, and the new Phase-2 machinery (identity, resolution, ledger, framework patterns). Analytics stay on-top in `codegraph/` (RFC-0012); persistence stays in the knowledge store adapters.
- **Tradeoff accepted:** tree-sitter grammar dependencies — today confined to an adapter — become dependencies of a `core/` crate. This is a deliberate exception to "parsing lives in adapters", justified below.
- **Revisit if:** a second parsing engine (LSP, compiler-backed) arrives and the crate's grammar set becomes a swapping surface — at that point the parser moves behind a port with grammar sets as adapter cells (ADR-0022 pattern).

## Context

Code ingestion currently lives in `adapters/ingest`: `tree_sitter_chunker.rs` holds the grammars, `extractor.rs` builds entities/relationships, `code_symbol.rs` provides the line-based fallback, and `scanner.rs`/`reconcile.rs` own orchestration and retraction. RFC-0020 Phase 1 (extraction-quality) shipped bare logical identities and suffix resolution; Phase 2 — receiver-qualified identity, a scope-wide multi-candidate symbol table (replacing the last-write-wins `name_index`, `extractor.rs:477`), typed code edges, an unresolved-reference ledger — makes the extraction logic substantially larger and more domain-shaped. Meanwhile the comparison with `@colbymchenry/codegraph` (docs/research §7) showed the extraction layer is where engram lags: an adapter is the wrong home for behavior that large.

The reference architecture (`docs/architecture/reference.md`) prescribes behavior crates as "service + ports for one concern each" depending only on `engram-domain`/`engram-runtime`, with adapters owning replaceable infrastructure behind traits. Code extraction is deterministic behavior over bytes — no I/O, no storage, no network — which is behavior-crate shaped, not adapter-shaped. The one mismatch: the convention so far has been that parsing libraries (tree-sitter) live in adapters.

## Decision

> `engram-code` is a behavior crate at `core/code`, depending only on `engram-domain` and `engram-runtime`. It owns the deterministic code-extraction pipeline: tree-sitter parsing (grammars are in-crate dependencies), symbol identity, typed-edge extraction, cross-file resolution with the unresolved-reference ledger, and framework pattern resolvers. Its public API is pure: `parse(path, language, source) -> CodeModel` and `resolve(models) -> ResolvedCodeGraph` — no filesystem, git, storage, async runtime, or LLM.

- **Tree-sitter in a core crate is the recorded exception.** Tree-sitter grammars are pure in-process parsing libraries — no I/O, no services, no engine lock-in — closer to serde than to sqlite. The neutrality rules that matter (ADR-0022: no engine types, no SQL, storage behind ports) are unaffected: parsing output is engine-neutral domain objects. The alternative — a `CodeParser` port with grammars in an adapter — adds a port boundary whose only implementation would live in the same workspace forever (YAGNI), and would split identity/resolution (which need AST context) from parsing across a crate boundary.
- **`adapters/ingest` stays the I/O orchestrator**: filesystem/git scanning, content hashing, retraction sequencing (reconcile), and calls into `engram-code` for parse + resolve. It keeps no tree-sitter dependency.
- **Analytics stay on-top** (RFC-0012): `codegraph/queries` and `codegraph/temporal` consume the graph `engram-code` produces; they are not absorbed.
- **Persistence stays in adapters**: entities, typed edges, and ledger rows persist through the existing knowledge store ports (`engram-knowledge`), extended per the engram-code spec.

## Alternatives considered

- **Keep extraction in `adapters/ingest`** (status quo): rejected — Phase 2's size makes the adapter a god crate; the AGENTS.md boundary rules push behavior out of adapters.
- **Parser port + grammar adapter cell** (full ADR-0022 pattern): rejected as premature — exactly one parser engine exists; the port's cost is paid now for a swap that may never come. The revisit trigger records when to reconsider.
- **Fold code extraction into `codegraph/` (on-top layer)**: rejected — `codegraph/` is queries-over-engram, not ingestion; moving extraction there inverts the dependency direction (RFC-0012).

## Consequences

- `core/code` joins the target repository shape; `adapters/ingest` shrinks to orchestration.
- The AGENTS.md repository tree gains a `core/code` entry; the boundary rules for the new crate mirror the other behavior crates (no SQL/vector/async/LLM, focused modules, facade root).
- Grammar dependency surface becomes visible at the `core/` layer — additive languages remain an explicit decision (the engram-code spec gates them behind Ask-first).
