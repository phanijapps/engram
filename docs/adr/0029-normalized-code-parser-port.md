# ADR-0029: Code parsers return normalized facts

- **Status:** Accepted
- **Date:** 2026-10-14
- **Decision-makers:** phanijapps
- **Supersedes:** ADR-0028 (parser-port deferral only)
- **Related:** [RFC-0021](../rfcs/0021-structural-code-context.md), [ADR-0028](0028-engram-code-crate.md), [ADR-0022](0022-engine-grid-vs-backend-recipe.md)

## Context

Engram's code chunker exposes Tree-sitter-specific parsing entry points.
The accepted RFC-0021 requires adding languages and future parser engines
without branching shared context, summary and retrieval consumers by language.
ADR-0028 placed deterministic code behavior in `engram-code` and deferred a
parser port until a second parsing engine arrived. That deferral is revisited
because additive parser support is now a required interface, not an assumed
future convenience.

## Decision

Code parsers implement a port owned by `engram-code` and return a shared,
language-neutral representation of units, source spans, structural facts,
diagnostics and feature coverage.

Language identifiers are open validated strings, not a closed language enum.
Providers map their syntax into typed common roles/facts; unsupported features
are explicit and source remains recoverable. Engine objects and mandatory
grammar-specific fields do not cross the port. Optional diagnostic labels do
not drive shared behavior. Host registration governs deterministic selection,
file discovery, dependency construction and enforceable resource limits.

`engram-code` continues to own deterministic identity, normalization validation,
coverage, segmentation and resolution. The built-in pure Tree-sitter provider
can retain grammar dependencies during the first cutover. On introduction of
a second parsing engine, parser implementations move into focused adapter
cells behind the port; process/network/filesystem I/O stays outside core.
Analytics stay in `codegraph/`, persistence behind knowledge ports, and LLM
integration in TypeScript. These ownership rules from ADR-0028 remain in force.

This decision partially supersedes ADR-0028's port deferral. It does not retire
the code behavior crate, change entity identity or install a parser/language.

## Decision drivers

- Adding a language requires a provider/mapping, registration and conformance
  fixtures, without changes to shared downstream consumers.
- Pure behavior remains testable without filesystem, storage or models.
- Per-feature support is distinguishable from mere grammar availability.

## Consequences

A test provider can exercise language extension independently of Tree-sitter.
Context and summaries share source/signature/parent/fact concepts across
languages. Provider mapping and conformance introduce additional contracts and
maintenance; language-specific semantics can still require reviewed typed
extensions rather than an opaque metadata escape hatch. The port is paid for
before a second engine exists, reversing the earlier YAGNI tradeoff.

**Revisit if:** a conforming new language requires language-ID branches in
shared consumers, or normalized facts cannot preserve needed semantics.

## Confirmation

- **Mode:** architecture fitness test
- **Signal:** a test-only new provider passes shared source/context consumer tests without editing those consumers; unsupported features are reported and accepted source is retained.
- **Owner:** phanijapps

These are implementation gates, not claims that the provider port is shipped.

## Alternatives considered

- Continue with Tree-sitter-specific APIs and defer the port: lower immediate
  cost, but does not establish the approved extension contract.
- Standardize a complete syntax tree: exposes more grammar detail and makes
  shared consumers responsible for language traversal.
- Place the port in ingest or the SDK: ties pure behavior consumers to I/O or
  transport composition rather than the code behavior boundary.

## References

- [RFC-0021](../rfcs/0021-structural-code-context.md), D1 and provider conformance gate.
- [Chunking audit](../research/ast-aware-code-chunking-audit.md).
- Acceptance records the user's explicit approval of RFC-0021 and its next-step sequence on 2026-10-14; implementation status is tracked by follow-on specs.
