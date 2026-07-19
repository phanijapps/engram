# RFC-0010: Decompose god modules by responsibility

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-05
- **Decision weight:** standard
- **Related:** AGENTS.md (god-class/module/package rules; facade rule), RFC-0006 / ADR-0010 (behavior-port split), `docs/specs/workspace-responsibility-layout`, `docs/specs/workspace-architecture-alignment`, RFC-0009 (KnowledgeRepository god-trait)

## Reviewer brief

- **Decision:** Adopt a concrete **decomposition doctrine** — split a module by its *reasons to change* (the decisions that vary), not by processing steps — and apply it first to **ingestion**, splitting the source-type-varying work (code / docs / structured / contract) behind a common `SourceExtractor` trait over a shared, source-agnostic spine.
- **Recommended outcome:** accept.
- **Change if accepted:** (1) a stated doctrine + a named, sequenced target list of god modules; (2) ingestion's per-source-type **extraction** (with per-type chunk-strategy selection) moves behind a common trait so a new source type (e.g. Excel, DB rows) is a new impl, not edits to `scanner.rs`/`extractor.rs`; (3) the ingest crate's public facade is preserved.
- **Affected surface:** `adapters/ingest` (internal split; facade unchanged) first; `adapters/knowledge/sqlite` and `bindings/node` sequenced as follow-on specs. No domain/contract change.
- **Stakes:** reversible and behavior-preserving — each split lands behind stable re-exports with no public-interface change; the ingestion split is a real refactor of the scanner's *scattered* kind-dispatch (not a trivial move), gated by the existing tests.
- **Review focus:** (a) D1 — is *source type* the right decomposition axis for ingestion (vs pipeline stage)? (b) D2 — modules-behind-a-trait now vs sub-crates now?
- **Not in scope:** rewriting the tests (the unit-inline / integration-in-`tests/` split is correct and stays); designing every target's split in this RFC; any behavior change.

## The ask

**Recommendation (BLUF):** Approve a decomposition doctrine — *split a module by its reasons to change (Parnas: the decisions that vary), keep crate roots as facades* — and apply it first to **ingestion**: move the source-type-specific **extraction** (with per-type chunk-strategy selection) behind a `SourceExtractor` trait with `code`/`docs`/`structured`/`contract` implementations, leaving walk/git/orchestration-dispatch/persistence/reconciliation as a shared spine. Sequence the other named god modules (`knowledge/sqlite/service.rs`, `bindings/node/lib.rs`) as follow-on specs.

**Why now (SCQA):**
- *Situation.* AGENTS.md already forbids god classes/modules/packages and mandates splitting a module that "mixes multiple reasons to change"; the repo has split-by-responsibility before (RFC-0006/ADR-0010, the workspace-responsibility-layout spec).
- *Complication.* Ingestion has crept back into a god shape: one crate walks files, detects git, chunks (plain/code/tree-sitter), extracts **code symbols + prose concepts + OpenAPI contracts**, orchestrates, and reconciles — with the source-type choice hard-branched across `scanner.rs` and `extractor.rs`. Adding a *structured* source type (Excel, DB rows) means editing those shared files' `match kind` arms — an Open/Closed violation. `contract.rs` (1047 lines), `scanner.rs` (994), `extractor.rs` (519) are the symptom; `knowledge/sqlite/service.rs` (1136) and `bindings/node/lib.rs` (1472) are the same disease elsewhere.
- *Question.* What decomposition axis makes source types independently addable and testable, what boundary mechanism, and in what order across the repo's god modules?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Ingestion decomposition axis | By **source type** behind a `SourceExtractor` trait; shared source-agnostic spine | The varying decision is the source type (Parnas); a new type = a new impl, no edits (OCP) | this review | Confirm axis; rule against by-stage |
| D2 | Boundary mechanism | **Focused modules behind the trait** now; sub-crates when the set grows | `adapters/ingest` is already a focused crate (AGENTS.md "prefer small crates" is satisfied); Rust Book: modules first, extract crates as it grows | this review | Confirm modules-now (crates deferred to a promotion trigger) |
| D3 | Targets + sequence | (1) ingestion (this RFC), (2) `knowledge/sqlite/service.rs`, (3) `bindings/node/lib.rs` | Largest / responsibility-mixing offenders; RFC-0009 already flagged #2 | this review | Confirm the target set + order |
| D4 | Scope of this RFC | Decide the doctrine + ingestion direction; sequence the rest as follow-on specs/ADRs | An RFC states direction; concrete splits are specs | this review | Confirm scope |
| D5 | Migration / compatibility | Preserve each crate's public facade; split internals behind stable re-exports; incremental | No consumer (demo backend) change; reversible | this review | Confirm facade-preserving stance |

## Problem & goals

**Diagnosis.** God modules have re-accreted despite the AGENTS.md rule. Measured (lines, `src/` only): `bindings/node/lib.rs` 1472, `knowledge/sqlite/service.rs` 1136, `ingest/contract.rs` 1047, `ingest/scanner.rs` 994, `ingest/extractor.rs` 519. Ingestion is the clearest *reasons-to-change* mix: file-walking, git detection, three chunking strategies, three extraction kinds (code symbols, prose concepts, OpenAPI contracts), orchestration, and reconciliation live together, and the source-type choice is a hard branch (`extractor.rs:108` `is_code = matches!(kind, Code)`; `scanner.rs` `match kind`). Each new source type or each new extraction concern edits the same shared files — the churn AGENTS.md's rule exists to prevent.

**Goals.**
- A stated, repeatable doctrine for when and how to split a god module (by reason-to-change / decisions-that-vary), so the rule is actionable, not aspirational.
- Ingestion: source types are focused, independently-testable units; adding one (structured: Excel, DB rows) is a new implementation behind a trait, touching no existing source type.
- A named, sequenced target list so the cleanup is finite and prioritized.
- Zero public-interface or behavior change; each split lands behind the existing facade.

**Non-goals.**
- Rewriting the test layout — the unit-inline / integration-in-`tests/` split is idiomatic and correct; it stays.
- Designing the concrete split of every target here (those are follow-on specs).
- Any behavior change, new domain type, or new external dependency.
- A line-count lint as the definition of "god" — line count is a *smell that prompts review*, not the criterion; the criterion is mixed reasons-to-change.

## Proposal

**Doctrine (applies to every target).** Split a module when it owns more than one *reason to change* — a distinct decision that varies (a source type, a storage backend, a transport, a scoring rule). Modularize around the varying decision (Parnas), expose new variants by adding an implementation rather than editing existing code (OCP), and keep the crate root / package entry a thin facade of re-exports (Rust Book; AGENTS.md). Line count is a review trigger, not the test.

**Ingestion (D1/D2) — the worked example.** Introduce a `SourceExtractor` trait owning the source-type-varying **extraction** step (entity/relationship extraction for one `SourceDocumentKind`, plus selecting the chunk *strategy*). Chunking itself is already abstracted behind the `Chunker` trait (`chunker.rs:26`; `KnowledgeIngestor<C>` is generic over it, `ingestor.rs:27`), and the shared plain-text chunker is reused by the spine — only `code` needs the tree-sitter chunker, which its extractor selects. The trait does **not** re-abstract chunking. Focused implementations:
- `code` — code-symbol + call/mention extraction (today's tree-sitter / `code_symbol` path),
- `docs` — prose concept/mention extraction,
- `structured` — Excel / DB-row extraction (new; the capability this unlocks),
- `contract` — OpenAPI contract extraction (today's `contract.rs`, itself split into parse / entity-build / persist-reconcile).

The **shared spine stays source-agnostic**: file walking, git detection, persistence, and per-source reconciliation; the orchestration loop's currently-scattered kind-branching is consolidated into a single dispatch to the `SourceExtractor` by kind. Realized first as focused **modules behind the trait** inside `adapters/ingest`. This satisfies AGENTS.md's "prefer small crates with explicit responsibilities" rule: `adapters/ingest` is *already* a focused adapter crate, so intra-crate modules refine it rather than growing a large shared crate; promotion to **sub-crates** (`engram-ingest-{code,docs,structured}`) follows the Rust Book's "extract crates as the package grows" only when a hard compile/ownership boundary is warranted (Open question 1). The crate's public facade (`scan_repository`, `GraphExtractor`, `DocumentIngestRequest`, …) is preserved.

**Targets + sequence (D3).** (1) ingestion, above; (2) `knowledge/sqlite/service.rs` — split the god-adapter along the god-trait seam RFC-0009 already named (source/document/chunk vs entity/relationship vs graph/ontology vs concept vs delete/reconcile); (3) `bindings/node/lib.rs` — split the N-API facade by exposed surface. `core/belief/reconcile.rs` and `memory/sqlite/retrieval.rs` are monitored, not yet targeted.

**Scope + migration (D4/D5).** This RFC decides the doctrine and the ingestion direction; each concrete split is a follow-on spec, landed as a facade-preserving internal refactor (behavior-preserving, gated by the existing tests). No public interface changes.

## Options considered

**Axis 1 — ingestion decomposition** (MECE along *what dimension is the reason-to-change*):

| Option | Split by | Trade-offs | |
| --- | --- | --- | --- |
| **A. Source type** (code/docs/structured/contract) behind a trait, shared spine | the varying *input* | New type = new impl (OCP); isolates the part that actually varies; the code already branches on kind | ★ recommended |
| B. Pipeline stage (read→chunk→extract→persist) as the primary split | the processing *steps* | Stages are shared across all types, so this splits the flowchart (the Parnas anti-pattern) and doesn't isolate the varying decision | |
| C. Do-nothing | — | Zero cost now; but every new source type / extraction concern keeps editing `scanner.rs`+`extractor.rs`, the god shape worsens, and structured ingestion is bolted onto the branch | |

**Axis 2 — boundary mechanism** (MECE along coupling/granularity): (a) **modules behind a trait** in one crate ★; (b) **sub-crates** per source type; (c) **runtime plugin registry**. Recommend (a) now (Rust Book: modules first), (b) as the set grows (Rust Book: extract crates as a package grows; the behavior-port-split precedent), (c) rejected as premature.

Prior-art grounding: A is Strategy + OCP (interchangeable per-type extractors behind one interface; a new type adds an impl); B is the flowchart decomposition Parnas warns against; C is the status quo. The Axis-2 boundary options track the Rust Book's modules→crates progression.

## Risks & what would make this wrong

**Pre-mortem.**
- *Abstraction with one implementation.* A `SourceExtractor` trait that only ever has `code` would be ceremony. **Mitigation:** the trait is justified *now* — there are already three extraction kinds (code, prose, contract) branched in-line, plus a fourth (structured) motivating it; the trait replaces existing branches, it doesn't anticipate hypotheticals.
- *Refactor churn / regression.* A behavior-preserving split can still break behavior. **Mitigation:** each split is gated by the existing unit + integration tests (100+ in ingest); no split lands red.
- *Facade drift.* Splitting internals could leak new public API. **Mitigation:** D5 fixes the facade as the invariant; the split is internal.
- *Over-splitting.* Decomposing past the point of cohesion. **Mitigation:** the doctrine is reason-to-change, not line count; stop when each unit answers to one reason.

**Key assumptions (falsifiable).**
- The scattered source-type dispatch (`scanner.rs:456,489,497,518,529,583`; `extractor.rs:108`; the `filesystem.rs:278` classifier) can be **consolidated** behind one `SourceExtractor` trait, leaving walk/git/persist/reconcile source-agnostic — verified as a bounded set of branch sites (a real scanner refactor, not a pre-isolated seam).
- The splits are behavior-preserving refactors — assumption; the test suites are the guard.
- Modules-first (not crates-first) is right — holds unless a hard compile/ownership boundary is needed sooner.

**Drawbacks.** A trait indirection over what is today a direct `match`; a migration cost per target; the doctrine adds a review step. Accepted: the indirection buys OCP extensibility (the whole point), and the targets are finite and sequenced.

## Evidence & prior art

**Spike / de-risk result.** Riskiest assumption: that ingestion's source-type variance can be moved behind one dispatch. Checked against the code — the branching is **not** pre-localized; it is smeared across the scanner's per-file orchestration loop (`scanner.rs:456` FileKind→SourceDocumentKind, `:489`/`:497` `match kind` dispatching the code- vs text-ingestor, `:518`/`:529` the code-only tree-sitter/AST path, `:583` inline contract extraction), plus the `is_code` branch in `extractor.rs:108` and a *second, richer* classifier in `filesystem.rs:278` (`classify_document` → Text/Markdown/Html/StructuredData/Code). So the seam is real but **wider than a single branch**: the ingestion split's substantive work is *consolidating* this scattered kind-dispatch behind the `SourceExtractor` trait, not relocating one pre-isolated function. The spine that stays source-agnostic is file-walk, git detection, persistence, and reconciliation; the orchestration *dispatch* is what the trait absorbs. Sub-finding: `contract.rs` (1047 lines) is itself a mini-god (parse + entity-build + persist + retract) — a split target within the example.

**Repo precedent.**
- **AGENTS.md** — the god-class/module/package prohibition, the "split by boundary" list, and the facade rule this RFC operationalizes.
- **RFC-0006 / ADR-0010** (behavior-port split) and **`workspace-responsibility-layout`** / **`workspace-architecture-alignment`** — the repo already split-by-responsibility into focused crates behind facades; this extends that inside a crate and by source type.
- **RFC-0009** — named `KnowledgeRepository` a god-trait; its 1136-line impl is target #2.

**External prior art** (fetched; each claim confirmed in the source or a reputable secondary — see the per-item notes).
- SRP — [R. C. Martin, "one and only one reason to change" / one actor per module](https://blog.cleancoder.com/uncle-bob/2014/05/08/SingleReponsibilityPrinciple.html).
- Information hiding — [Parnas 1972: begin decomposition from "design decisions likely to change," not a flowchart](https://blog.acolyer.org/2016/09/05/on-the-criteria-to-be-used-in-decomposing-systems-into-modules/) *(confirmed via a reputable secondary source — the primary copies were unreachable/blocked; a library PDF should replace this in the final bibliography)*.
- OCP — [open for extension, closed for modification; the polymorphic/abstract-interface reformulation](https://en.wikipedia.org/wiki/Open%E2%80%93closed_principle).
- Package-by-feature (high cohesion, low coupling; "adding new features is easy") — [Tennakoon, "Package by Features or Package by Layers"](https://chathurangat.wordpress.com/2017/09/12/software-architecture-package-by-features-or-package-by-layers/) *(blog; the canonical primary source had an expired certificate)*.
- Rust — [The Rust Book, Ch. 7: split into modules then extract crates as a project grows; the crate root as an encapsulating facade](https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html).

## Open questions

1. **Sub-crate promotion trigger.** (owner: ingestion-split spec author; decide-by: that spec). D2 settles modules-behind-a-trait as the first step; the open part is *when* to promote to `engram-ingest-{code,docs,structured}` sub-crates. Recommended default: promote only when a hard compile/ownership boundary or independent versioning is needed — not before.
2. **A "god-module" review trigger in CONVENTIONS.** (owner: you; decide-by: on acceptance). Recommended default: add a *review prompt* (a file mixing ≥2 reasons-to-change, or crossing a size heuristic, gets a decomposition look) — explicitly not a hard line-count gate.
3. **`contract.rs` split shape.** (owner: ingestion-split spec author; decide-by: that spec). Recommended default: split into parse / entity-build / persist+reconcile within the `contract` source-type unit.

## Follow-on artifacts

*Filled in on acceptance.*
- ADR: record the decomposition doctrine (reason-to-change + source-type-behind-a-trait + facade) and the target sequence.
- Specs: `docs/specs/ingest-source-type-split/` (the `SourceExtractor` trait + code/docs/contract modules, structured stubbed), then `knowledge-sqlite-adapter-split` and `node-binding-facade-split`.
- Possible `docs/CONVENTIONS.md` addition: the god-module review trigger (Open question 2).
