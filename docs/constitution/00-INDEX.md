# Engram Software Constitution — Master Index

Status: ratified 2026-10-31 (owner approval, docs/research/software-docs/constitution/00-sdd-constitution-plan.md §7).
Implemented same day: L1 structural conformance verified by SCCT reference
checker (node scct/scct.mjs from Software-Constitution/spec) — all artifacts
found, registry resolved, 9 numbered rails. L2 not asserted: gate-enforcement
gaps are recorded per-rail; when the planned rule-class gates land (month 1),
naming them to match SCCT filename patterns (e.g. check-no-orphans.py) earns
the L2 badge — discipline first, badge second.
Standard: software/constitution™ — pinned to SPEC.md §4 v1.0 (canonical:
https://raw.githubusercontent.com/Software-Constitution/spec/main/SPEC.md). The site
v1.1 and org README disagree on rule-class count (six vs five); this repo pins to the
SPEC.md six-class set. No conformance level is asserted yet; see the roadmap in
docs/research/software-docs/constitution/01-constitution-organization.md §5.

- Spec root: `docs/contracts/docs/constitution/v1`
- Gates root: `tools/ci`
- Scope: Rust library, TypeScript SDK/native binding, unified MCP server, and
  repository-owned distribution/agent-bundle surfaces.

## The project in one page

Engram is a contract-first, local-first agentic memory and knowledge engine:
governed **memory**, source-grounded **knowledge** (graph + ontology packs),
derived **beliefs** (valid-time), and generation-bound **code intelligence** —
Rust deterministic core, TypeScript ergonomics, one MCP surface, installable
agent bundle. NOT an LLM host, vector DB, or app server. Full domain
knowledge lives in the packs below; extended history in `docs/product/` and
`docs/architecture/`.

## Universal rules

Every rail below is subordinate to these. RFC 2119 keywords apply.

- RULE-1 (single implementation): every named capability MUST have exactly one canonical
  implementation; transports and wrappers MUST import or delegate to it, never reimplement.
- RULE-2 (distribution drift): every declared shipped artifact or managed installation
  MUST reconcile to a recorded source revision. For this library/SDK repo, "prod" means
  published surfaces — npm package contents, crates.io publications, and the installed
  agent bundle (`packages/launcher/skills+agents` vs `.agents/skills`+`mcp/engram-mcp/agents`).
- RULE-3 (production completeness): production-reachable code MUST NOT contain stubs,
  placeholders, `TODO`/`FIXME`, or mock behavior. Test fixtures live in test directories.
- RULE-4 (canonical evidence): privileged actions (job approve/rework/release, deviation
  open/close) MUST emit canonical evidence events; event families MUST be registered in
  `docs/contracts/docs/constitution/v1/events/registry.json` before any code references them.
- RULE-5 (zero deviations): `DEVIATIONS.md` `## Active deviations` MUST be empty;
  non-emptiness is itself a gate failure. Time-boxed rule clauses live in
  `DECAY-WINDOWS.md`, never in DEVIATIONS.
- RULE-6 (no orphans): every named component (crate, package, transport, gate, registry)
  MUST be declared in `components/registry.json` and every declaration MUST resolve to a
  path on disk — both directions.

## Permanent rail numbers

Numbers are permanent once locked. Amendments preserve rule identifiers and record
supersession in the rail's history block.

| Rail | Rule classes | Gate ownership |
|---|---|---|
| [01 Engine neutrality](01-engine-neutrality.md) | RULE-1, RULE-6 | check_engine_neutrality.py (existing) |
| [02 Surface parity](02-surface-parity.md) | RULE-1, RULE-6 | check_surface_parity.py (existing) |
| [03 Canonical implementations](03-canonical-implementations.md) | RULE-1 | check_implementation_registry.py (planned) |
| [04 Distribution drift](04-distribution-drift.md) | RULE-2 | check_distribution_drift.py (planned) |
| [05 Production completeness](05-production-completeness.md) | RULE-3 | check_production_completeness.py (planned) |
| [06 Canonical evidence](06-canonical-evidence.md) | RULE-4 | check_canonical_evidence.py (planned) |
| [07 Zero deviations](07-zero-deviations.md) | RULE-5 | check_constitution.py (existing, this adoption) |
| [08 Coherent integration](08-coherent-integration.md) | RULE-6 | check_coherent_integration.py (planned) |
| [09 Principles (tie-breakers)](09-principles.md) | values — cross-class (RULE-1, RULE-4) | check_contracts.py + check_architecture_guards.py (existing); dependency lint (planned) |

## Canonical structured declarations

- Implementation ownership: `docs/contracts/docs/constitution/v1/implementation/registry.json`
- Event families: `docs/contracts/docs/constitution/v1/events/registry.json`
- Rule-to-gate mapping: `docs/contracts/docs/constitution/v1/gates/registry.json`
- Distribution declarations: `docs/contracts/docs/constitution/v1/distributions/registry.json`
- Component inventory: `docs/contracts/docs/constitution/v1/components/registry.json`
- Validation schemas: `docs/contracts/docs/constitution/v1/schemas/`

## Change and exception policy

- A rail number MUST remain permanent once locked; amendments preserve rule IDs and
  record supersession.
- A constitution change MUST update its gate mapping and acceptance evidence in the same
  review.
- DEVIATIONS.md records scoped, approved, expiring exceptions; DECAY-WINDOWS.md records
  temporal clauses of rules, not waivers. Expiry MUST close the window or fail CI; neither
  a script allowlist nor an ADR alone is a green bypass.
- Every declared rule MUST have a blocking gate before L2 is asserted; gates run on every
  push and pull request once wired.
- No passing syntax check substitutes for behavioral evidence.
- The SDD pipeline (brief → analysis/spec → plan → tasks → verification) is governed by
  the same constitution: artifact frontmatter and ID integrity are checked by
  `tools/ci/check_job_artifacts.py` (advisory until one clean job cycle completes,
  then blocking).

## Domain sub-constitutions (ratified 2026-11-01)

Seven subordinate packs — product, architecture, build, testing, ci, security,
release — each with its own `00-INDEX.md`, locally-prefixed rule IDs, and
typed references into root rules (governs/refines/enforced-by). Root rails
01-09 remain intact and authoritative. Machine catalogue:
`docs/contracts/constitution/v1/constitution-catalogue.json`. ci/ is an
enforcement view only. Plan: docs/research/software-constitution/13-federated-constitution-plan.md.

## Amendment record

- 2026-10-31 (later): rail 09 (Principles) added — the six tie-breaker values
  absorbed from the retired `docs/CHARTER.md`, corrected to current reality
  (pgvector shipped; embedding scheduling per ADR-0024) and given honest gate
  mapping (PR-003 already enforced by check_architecture_guards.py).
- 2026-10-31 (owner direction): constitution/ relocated to `docs/constitution/` and
  contracts/ to `docs/contracts/` — docs/-centralized layout. This is a deliberate,
  recorded deviation from SPEC.md §3's repo-root artifact paths; the SCCT reference
  checker will report L0 for this repo by layout choice, not by missing discipline.
  Internal gates (`tools/ci/check_constitution.py`) enforce the same rules at the
  new paths.

## Division of labor

- `AGENTS.md` — how to work here (agent-facing guidance, validation commands).
- `docs/constitution/` — what is enforced at CI time (gate-backed, merge-blocking).
- `docs/adr/` — why a decision was made (context, alternatives, consequences).
- `docs/specs/` — feature acceptance criteria; specs carry constitution rule IDs.


## Root law — zero deviations (from 07) and principles (from 09)

### From 07-zero-deviations.md

# Rail 07 — Zero deviations

Rule class: RULE-5 (zero-deviation mandate).
Gate: `tools/ci/check_constitution.py` (existing as of this adoption; umbrella).

## Rules

- **ZD-001 (MUST)**: `docs/constitution/DEVIATIONS.md` `## Active deviations` MUST be empty;
  any heading under Active is itself a CI failure.
- **ZD-002 (MUST)**: Every active deviation MUST record: rule ID violated, scope,
  approver (owner), opened date, expiry date (≤ 90 days), and closure condition.
- **ZD-003 (MUST)**: An expired deviation MUST be closed (resolved and moved to
  Historical) or escalated to the owner the same day; expiry without closure fails CI.
- **ZD-004 (MUST)**: Temporal clauses of rules (measured baselines with closure
  trajectories) live in `DECAY-WINDOWS.md`, never in DEVIATIONS.

## Enforcement boundary

`check_constitution.py` fails when: any heading exists under `## Active deviations`; a
deviation entry lacks approver/expiry; a decay window is past its closure date without a
closure note.

## Policy

Deviation approval is owner-only. Default maximum concurrent active deviations: 3
(exceeding it fails regardless of individual validity).

## History

- 2026-10-31: rail locked at adoption; Active section empty (day one).

### From 09-principles.md

# Rail 09 — Principles (tie-breakers)

Rule classes: values rail — cross-class (RULE-1, RULE-4 flavored).
Owner: core maintainers. Origin: absorbed from the retired `docs/CHARTER.md`
(retired 2026-10-31 after scope drift; the principles below are the surviving,
corrected core). Principles resolve ties when reasonable people disagree; they
are constitution rules like any other — with enforcement stated honestly.

## Rules

- **PR-001 (MUST) — Contract first.** The domain model and its JSON contracts
  are the source of truth; Rust types and generated TypeScript conform to them,
  never the reverse. `docs/architecture/domain-data-model.md` outranks a
  convenient implementation. Generated artifacts MUST be reproducible from
  source and MUST NOT be hand-edited.
  Gate: `tools/ci/check_contracts.py` + `tools/contracts/validate_contracts.py`
  (existing, blocking) + the CI generated-drift diff.
- **PR-002 (MUST) — Rust owns the deterministic core; TypeScript owns
  ergonomics.** Behavior that must be reproducible lives in Rust behind traits;
  integration glue and any LLM client live in TypeScript. Neither re-implements
  the other. LLM/model SDK crates MUST NOT appear in any `core/*` or
  `adapters/*` Cargo manifest.
  Gate: review today; dependency-direction lint is the planned month-1 gap
  (candidate: fold into `check_production_completeness.py`).
- **PR-003 (MUST) — Small crates, explicit responsibilities.** No god modules
  or god packages; a file mixing construction, validation, state, scoring, and
  persistence is split before handoff. Crate roots are facades; behavior lives
  in focused modules.
  Gate: `tools/ci/check_architecture_guards.py` via `check_docs.py`
  (existing, blocking — crate-root line caps and module responsibilities).
- **PR-004 (MUST) — Infrastructure lives behind adapters.** Storage, vectors,
  embeddings, and models sit behind traits so tests use deterministic stubs and
  engines are swappable cells on the capability × engine grid (ADR-0022);
  pgvector is a shipped engine, not an exception.
  Gate: `tools/ci/check_engine_neutrality.py` (existing, blocking) +
  `docs/contracts/constitution/v1/implementation/registry.json` drift gate
  (planned).
- **PR-005 (MUST) — Policy is visible on every path.** Scope, retention, and
  allowed-uses checks appear on write, retrieve, ingest, consolidate, and forget
  — never hidden inside a generic manager.
  Gate: review + conformance suites today; a policy-presence lint over the five
  paths is a candidate month-1 addition.
- **PR-006 (SHOULD) — Measured over intuited.** Scheduling and performance
  decisions (embedding generation timing, reindex batching per ADR-0024, fusion
  weighting) follow benchmarks and fixtures, not intuition. The original
  charter's "lazy embeddings" phrasing is retired with it: the principle is the
  discipline, not any single mechanism.

## Deviation path

Same as every rail: fix first; otherwise a scoped, owner-approved, expiring
DEVIATIONS entry while RULE-5 stands. A principle that has stopped resolving
ties — ignored, or contradicting another unacknowledged — is raised to the
owner, not silently edited.

## History

- 2026-10-31: rail created, absorbing the surviving six principles from the
  retired charter (scope drift documented in docs/README.md Housekeeping).

