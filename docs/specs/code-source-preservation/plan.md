# Plan: Code source preservation

- **Spec:** [`spec.md`](spec.md)
- **Status:** Complete

## Approach

Deliver the source-coverage correction independently before snapshot activation.
Settle portable record and file-generation contracts first, then land knowledge
readers/eligibility and recovery controls, then turn on snapshot writes. Reuse
existing parser functions while adding typed coverage output; the full parser
registry belongs to the next spec. Activation remains BestEffort across stores
and temporarily excludes only the updating file. Implementation follows the approved task order.

## Constraints

RFC-0021 D2 and its activation/compatibility rules constrain the slice.
ADR-0029 owns parser normalization; ADR-0030 separates source from derivation.
ADR-0028's remaining pure-code ownership and ADR-0022 neutrality/parity apply.
`AGENTS.md` takes precedence over historical reference-architecture descriptions
of retired facades, backend availability and old hook paths.

Before implementation run `scripts/ci/check-contracts.sh` and
`scripts/ci/check-engine-neutrality.sh`. Use work-loop's full mode for the
contract/persistence/transport changes; do not run writer tasks ahead of their
reader dependencies. Generated contracts are never edited manually.

## Construction tests

Cross-task fixtures include the audit's exact UserService/checkout examples,
Rust/Python equivalents, dirty/non-git source, Unicode/CRLF/BOM, empty source,
import-only files, recovered syntax and filtered files. A conformance flow
scans, reads/reconstructs snapshots, retrieves a semantic unit, re-scans with
changed content/fingerprint, injects partial failure, retries and removes a file.
A sibling file remains available throughout. A poisoned snapshot text must not
become a normal retrieval/model-document input.

Live verification uses `scripts/dev/mcp_driver.py` against a dedicated temporary
store, with the built binary from this implementation. Model/vector lanes are
only claimed verified when actually enabled; deterministic embedding stubs prove
candidate selection separately. Supported backend conformance includes SQLite
and pgvector recipes, with unsupported cells reported rather than fake success.

## Design (LLD)

### Interfaces & contracts

Traces to AC4–AC11. Existing `KnowledgeRepository`, `KnowledgeQuery`,
`EngramProvider` and scan entry points remain the transport seams. Add a narrow
knowledge-owned generation/source-read port; integration composes it and N-API
only translates. Exact names/signatures and schemas are settled in T0 and
recorded in the domain model and accepted contract, not a parallel TS model.
The source-read port takes a separate trusted effective scope; hosts must resolve
it from launch/requester authority, never from the file key supplied on the wire.
Adapters compare every scope dimension before lookup. Construction tests mutate
all dimensions, including resolved nulls. Default unsupported implementations
return typed Unsupported and cannot enable
snapshot writers. No SQL/engine type enters these neutral interfaces.

Source-read is a new operation, not an existing API claimed to be wired. Current
Rust MCP registration/dispatch is `mcp/engram-mcp/src/main.rs` (`scan`, `code`)
with scan behavior in `codegraph.rs`. TS HTTP MCP registration is
`packages/runtime/src/mcp/tools.ts`, currently using granular `scan_repo` and
`symbol_context` operations, with helpers in `mcp/codegraph.ts`. T7 introduces
the same source-read operation on both while preserving their legacy aliases;
T0 fixes its negotiated schema/operation names before implementation. It does
not assume TS already has Rust's consolidated discriminators.

### Data & schema

Traces to AC1/AC3/AC4/AC6/AC9/AC10. Optional typed code descriptors on existing
knowledge records carry source role, byte coverage and generation. New byte
ranges are explicit UTF-8 end-exclusive spans; legacy `SourceLocation` meaning
is untouched. Snapshot chunks partition source bytes and have existing kind
CodeBlock, semantic units reuse existing kinds. Descriptor roles distinguish
snapshots from all normal retrieval inputs.

File generation uses a file-view key `(scope, repositoryKey, relativePath)`.
Do not reuse the repo-wide `stable_source_key` alone: current scanner and
`reconcile.rs` already distinguish files by path. The knowledge adapter stores
a typed authoritative manifest and CAS lease. Snapshot/unit IDs bind file-view,
content/fingerprint, role and actual spans; logical graph identity is unchanged.
Graph/index source-contribution references carry the view/generation needed for
read-time eligibility; a shared entity is not globally hidden by one owner.
ImportData is extended versionedly; export/import remains on existing services.

SQLite stores source-only records in internal record tables and exposes eligible
read views at the legacy knowledge table names. This centralizes snapshot and
generation filtering for candidate queries, direct graph projections and old
readers; the current adapter's writes and cleanup use internal tables. Legacy
binaries cannot write those views, and their schema/index initialization cannot
silently enable source-ignorant writers. Migration is local to the knowledge
adapter, transactionally renames only its own tables, preserves legacy data and
indexes, and adds no shared connection or cross-store transaction. Dedicated
source reads/export use explicit scoped internal reads; candidate views never
provide raw snapshots. A SQL-boundary test restricts candidate/resolver reads to
the views; internal record reads are limited to scoped source/export, mutation
and cleanup. Views enforce eligibility, while ordinary scope/policy checks still
authorize reads. The implementation tests both upgraded and legacy-shaped
reads/writes before producer activation.


### Component / module decomposition

Traces to AC1–AC8. Pure coverage/range and callable classification helpers live
in focused `core/code/src/parser/` modules, with narrow re-exports at crate root.
`adapters/ingest` owns strict byte admission and file-phase orchestration. Split
scan admission, snapshot assembly, generation coordination and manifest-version
handling into focused collaborators instead of enlarging `scanner/mod.rs`.
Knowledge adapters implement the generation port and durable records.
A typed source-write envelope checks the current updating attempt and staged
inventory in the same local transaction as each record write. Shared synchronous
CRUD helpers avoid duplicating SQL or async lock re-entry. Every source-owned
write, including cross-file reconciliation, uses this fence. Code physical
source/document/chunk IDs bind the full scope and file; logical symbol and
repository keys remain unchanged. Retrieval
adapters consume shared eligibility, not a second role-policy implementation.

### State & control flow

Traces to AC6–AC8. Admission and strict UTF-8 validation precede mutation. Acquire
CAS lease for the exact file; mark updating before the existing pre-write
retraction. Persist snapshot and semantic/graph inventory; validate complete
snapshot plus required writes; CAS active. Any required write/cleanup failure
leaves incomplete and does not advance the host manifest's successful skip
state. A newer attempt supersedes the lease. Retry reuses content-bound IDs,
revalidates inventory and activates without rollback of successful steps.

Reader eligibility is applied before fusion and rechecked before response.
Updating/incomplete file contributions are unavailable with source failures.
Historical reads require retained matching data. Snapshot-only records remain
excluded even after a source becomes active. Index failures can leave derived
lanes pending without exposing prior-generation projections.

### Failure, edge cases & resilience

Traces to AC4–AC10. Strict decoding rejects invalid UTF-8 before retraction.
Parser recovery preserves snapshots but reports incomplete semantic features.
Zero-byte snapshots avoid calling a chunker that rejects empty text. Missing /
corrupt old scan manifests trigger bounded rescan, not trust in an unchanged
legacy fingerprint. CAS conflicts and failed persistence never turn into an
empty successful read. Shared backend tables remain behind their capability
port; no ATTACH/shared-connection trick is used for cross-store atomicity.

Deletion/policy retraction excludes source contribution before cleanup. Cleanup
failure retains a retryable state and cannot be stamped unchanged. Versioned
readers/role selectors deploy before producer activation; see Rollout.

## Tasks

### T0: Portable source records and compatibility fixtures are defined

**Depends on:** none

**Spec mapping:** AC4, AC6, AC9–AC11. **Mode:** goal-based contract checks + TDD schema/serde fixtures.

**Tests:**
- Legacy JSON decodes; negotiated code descriptors and typed byte ranges validate.
- Unknown/invalid role/range/generation shapes fail; strict legacy projection omits new fields.
- File-view keys distinguish sibling paths and scopes; no graph-name changes occur.

**Approach:**
- Update `docs/domain-data-model.md`, compatibility/changelog and accepted wire schema first; use `pnpm run contracts:generate` for projections.
- Define optional code descriptors, coverage, generation manifest/CAS lease and source-read request/result with limits/policy semantics.
- Add portable records in focused `core/domain` modules and narrow knowledge/source-generation ports, with migration DTO extensions in `core/integration/src/migration.rs`.
- Keep role/generation absent on legacy records explicitly unknown; define minimum reader/index policy version and capabilities.

**Done when:** exact contract diffs are classified, generated checks and schema/serde/type fixtures pass; snapshot writers remain disabled.

### T1: Callables retain behavior around nested declarations

**Depends on:** none

**Spec mapping:** AC2. **Mode:** TDD pure/adapter integration.

**Tests:**
- Exact checkout fixture retains fee, error branch and return in its outer unit.
- Nested TS/Rust/Python functions/methods retain full outer bodies, multiline signatures and qualified child anchors.
- Class/impl containers keep existing independent method extraction without full-container duplication.

**Approach:**
- Replace blanket `has_declaration_descendant` suppression in `core/code/src/parser/tree_sitter_chunker.rs` with explicit callable versus structural-container treatment in a focused helper.
- Extend `adapters/ingest/tests/tree_sitter_chunker.rs` and focused code parser tests; leave public record fields unchanged in this independently shippable correction.

**Done when:** construction fixtures pass and a built live scan demonstrates the restored outer source. This task does not claim snapshot/whole-file coverage.

### T2: Accepted code reconstructs exactly from bounded snapshot ranges

**Depends on:** T0

**Spec mapping:** AC1, AC3, AC4. **Mode:** TDD pure range/coverage fixtures.

**Tests:**
- Source reconstruction equals original UTF-8 bytes/hash for all boundary fixtures; zero-byte input yields [0,0).
- Composite context ranges recover full multiline headers, fields/initializers/imports and unmapped residual code.
- No descriptor conflates full declaration span with shorter rendered context.

**Approach:**
- Add pure snapshot partition/range validation in focused `core/code` modules.
- Add candidate descriptor/coverage assembly without persistence or grammar dependencies beyond the existing set.
- Emit source-slice context/residual units with declared per-feature support; raw snapshot text has no synthesized prefix.

**Done when:** property/fixture tests prove byte reconstruction and coverage classification; production snapshot writes are still disabled.

### T3: Knowledge cells enforce file leases and source-read policy

**Depends on:** T0

**Spec mapping:** AC5–AC7, AC11. **Mode:** TDD backend conformance.

**Tests:**
- Concurrent leases, superseding attempts and failed CAS cannot activate older data.
- Authorized exact source reads succeed; wrong scope, denied policy and unsupported capability fail safely.
- An explicit generation resolves only matching retained active evidence; missing
  historical inventory fails unavailable. Omitted generation selects current active
  source. A request never silently receives another generation.
- Omitted/broad scope fields cannot match multiple workspaces/subjects; trusted resolution uses exact tuples and null is not a wildcard.
- Per-file updating excludes its contribution while siblings/shared independent owners stay eligible.

**Approach:**
- Implement the narrow port in focused knowledge adapter modules for supported recipes; compose handles in recipe bootstraps.
- Use each engine's local atomic update for manifest/CAS, not a transaction spanning vector/lexical stores.
- Reuse existing policy/scope helpers; define source/contribution ownership needed by graph readers.

**Done when:** recipe conformance passes or a cell reports Unsupported and cannot enable writers; all neutral interfaces remain engine-free.

### T4: Every candidate/model-document feed excludes snapshot and inactive data

**Depends on:** T0, T3

**Spec mapping:** AC5, AC6, AC9, AC11. **Mode:** TDD cross-lane selection + goal-based build.

**Tests:**
- Snapshot body with uniquely searchable text never appears in code/default recall, lexical/vector/graph/community feeds or automatic document-model input.
- Without explicit per-source/destination enrichment authorization, no code-source role or legacy code document enters a model feed; explicit raw reads do not supply that grant.
- Content-hash reuse cannot copy a snapshot embedding onto an eligible unit.
- Inactive-generation projections are filtered; a generation changing during hydration is reported as degraded.

**Approach:**
- Extend `ChunkRef`/lean listings with typed role/view/generation eligibility so filtering does not require full-text materialization.
- Reuse shared eligibility at index resolvers and composition; cover MCP `embed_pending`, SDK/N-API indexing, retrieval adapters and TS maintenance document reads.
- Enforce separate model-egress authorization; where unavailable, skip code documents in `packages/runtime/src/maintenance/extract_knowledge.ts` rather than treating raw readability as consent.
- Deploy negotiated projections/minimum-reader protections and capability states before producers.

**Done when:** a populated adapter fixture proves negative snapshot hits on every applicable lane; no writer is enabled yet.

### T5: Scanner writes and re-scans activate complete per-file source views

**Depends on:** T1, T2, T3, T4

**Spec mapping:** AC1, AC3, AC4, AC6–AC8. **Mode:** TDD scanner integration with failure injection.

**Tests:**
- Invalid UTF-8 is rejected before prior-view mutation; accepted empty/whitespace files retain snapshots.
- Snapshot/unit/graph and cleanup failures leave incomplete state and retryable host manifests.
- Repeating bytes/fingerprint converges; changing fingerprint requeues unchanged bytes.
- One changed file does not mark its entire repository unavailable.

**Approach:**
- Replace lossy decode in code admission and split orchestration responsibilities out of `adapters/ingest/src/scanner/mod.rs`.
- Coordinate lease/updating before `reconcile::delete_prior_graphs_for_path`; enforce failed-cleanup results instead of ignored deletion errors.
- Persist candidates through `KnowledgeIngestor`, verify inventory/coverage and CAS active.
- Version both MCP and N-API host scan manifests; maintain legacy manifest recovery and existing filters/default limit.

**Done when:** failure/retry/rescan tests pass and supported producers can be enabled without violating read eligibility.

### T6: Existing export/import preserves source descriptors and activation state

**Depends on:** T0, T3, T5

**Spec mapping:** AC7, AC9, AC10. **Mode:** TDD migration/export conformance.

**Tests:**
- Round-trip snapshot bytes, typed coverage/generation, policy and provenance through ImportData.
- After round-trip, uniquely searchable snapshot text remains absent from normal recall, embeddings and model-document candidates on the target store.
- Unsupported target/schema version fails validation; requested scope translation remains policy-safe.
- Missing target authorization, broader visibility/uses/destinations, discarded scope restrictions and unrepresentable policy mappings cannot activate imported views.
- Failed import cannot activate incomplete source inventory.

**Approach:**
- Extend existing export concrete-store methods and MigrationService mappings; reuse the format and validation pipeline.
- Include file-generation records through typed versioned DTOs and rebuild target state only after inventory checks.
- Apply source-export and target-write authorization with an explicit scope mapping; validate policy non-widening before target activation, including destination/retention constraints.
- Preserve retraction/deletion behavior, including all new source-only records.

**Done when:** conformance verifies full source round-trip and explicit unsupported mappings without new export SQL/protocols.

### T7: Source operations and capability states agree across transports

**Depends on:** T3, T4, T5, T6

**Spec mapping:** AC9–AC11. **Mode:** goal-based surface parity + vertical transport tests.

**Tests:**
- Equivalent Rust/N-API/TS/MCP source reads return identical bytes/coverage or equivalent typed failures.
- Legacy response projections remain valid; every advertised supported capability is callable.
- Unsupported recipe/old reader cannot enable a new writer.

**Approach:**
- Expose narrow integration facade operations, N-API JSON transport and generated typed TS wrappers.
- Wire Rust in `mcp/engram-mcp/src/main.rs`/`codegraph.rs` and TS HTTP in `packages/runtime/src/mcp/tools.ts`/`codegraph.ts`; source-read is new, with operation/schema fixed in T0 and current scan/symbol-context aliases preserved.
- Reuse `bindings/node/src/provider.rs` and `knowledge_chunks.rs` plus provider-backed TS transport; no stand-alone source server or second TS implementation.
- Update capability reporting, client type tests and TS package build when public surfaces change.

**Done when:** parity gates plus transport scenarios pass, with supported/unsupported claims matching actual wiring.

### T8: Live source workflows and release gates prove the slice

**Depends on:** T1-T7

**Spec mapping:** AC1–AC12. **Mode:** live manual/E2E verification + goal-based repository gates.

**Tests:**
- Fresh and legacy temporary stores run the UserService/checkout/polyglot scan, exact snapshot reads, search exclusion, rescan/fingerprint changes and removal.
- With real configured vectors, snapshot candidates stay absent; otherwise record vector verification as not exercised.
- Every AC is supported by a construction/vertical/live result or stays unchecked.

**Approach:**
- Build `engram-mcp` and drive real stdio calls through `scripts/dev/mcp_driver.py`; use actual facade/N-API calls for parity.
- Run cargo fmt/check, affected crate tests, pnpm contracts generation/typecheck/test/build as applicable, contract/docs/neutrality/parity gates.
- Complete adversarial, quality and boundary-scoped security review; record commands/counts and cleanup only dedicated test artifacts.

**Done when:** all criteria are evidenced, reviews are clean, spec becomes Shipped and capability/product/backlog documents reflect actual delivery.

## Rollout

T1 can land as a pure source-loss correction independently of the rest. T0/T3/T4
land compatible contracts/readers/role-aware selectors before T5's snapshot
producer. Activate migrated views per file after coverage verification; derived
index lanes may remain pending with reported source failures. Re-scan sources
with new fingerprints rather than bulk rewriting unknown legacy coverage.

No new infrastructure service, parser or LLM destination is needed. Old binaries
that cannot enforce typed roles/minimum-reader version must reject the migrated
store or use a compatible backup. Rollback disables new writers while compatible
readers remain; successful writes across cells are not rolled back as if ACID.
The dedicated test stores are disposable; agentzero/self-index stores are retained.

## Risks

- Existing retraction ignores some child deletion failures; activating despite
  them would leave orphan source/index records. T5 failure tests cover this path.
- Repo-wide source keys can mistakenly suppress sibling files; file-view keys
  and ownership-aware shared graph selection are mandatory.
- A role check only in MCP leaves SDK/TS ingestion/maintenance paths exposed;
  shared feed/resolver fixtures and parity gates prevent that shortcut.
- Snapshot duplication increases storage; measure retained byte counts and
  index entry counts without claiming a performance improvement.
- Current historical knowledge policies do not retain all old chunks; missing
  historical snapshots return unavailable rather than promise new retention.

## Changelog

- 2026-10-14: Initial implementation plan after RFC-0021 approval. T1 is an independently shippable source-loss correction; snapshot/contract/read-generation tasks retain full-slice dependencies and are not claimed implemented.

- 2026-10-14: Execution authorized. Existing spec/secure-design reviews are clean; full work-loop initialized. T1 regression red/green is recorded by the implementation worker. T0 materializes the code-source contract with an additional optional owning document kind to classify legacy model inputs without changing non-code document behavior.

- 2026-10-14: T3/T4 use adapter-owned eligible SQLite read views over internal record tables. Inspection showed that a version marker alone cannot protect old binaries that never read it; the view boundary also prevents snapshot/stale reads and legacy writes. Scope and schema migration remain local to the knowledge cell.

### Foundation verification (in progress)

- T1 checkout/nested callable regression: `cargo test -p engram-code` and
  ingestion chunker fixtures passed (outer source retains fee/error/return).
- T2 exact partition/context tests: red missing API; green 4 fixtures for
  UTF-8/BOM/CRLF/empty, overlapping coverage, multiline class fields and recovered syntax.
- T3 source cell tests: red missing port implementation; green 9 tests covering
  exact read/exclusion, exact trusted scope, private policy/read limits/history,
  ABA, corrupt/missing inventory, sibling/shared ownership and fenced cleanup.
  Required owning-document validation separately failed before its correction.
- T4 model feed: negative fixture observed 7 unauthorized code model calls;
  after exclusion all 76 runtime tests passed, including Markdown code examples.
  Legacy graph-cache regression initially lacked revision enforcement; green
  test now rejects an updating file's cached entity. Cross-lane checks continue.
- `cargo check --workspace` passes at this foundation checkpoint.
  Affected code, SQLite, associative and community crate tests pass.
- Snapshot scanner producers, export/import, facade/transport operations and
  rebuilt live verification are still required. No AC is checked complete yet.

- Foundation reviews caught and corrected residual emission, legacy ownership,
  attempt reuse, overlapping cleanup, and stale lexical projections. T3 now has
  13 passing tests; T4 cross-lane tests have 3 passing scenarios. Stored lexical
  projection text requires a durable old-index migration before producer rollout.
- T5 scanner tests are red: no source generations are produced yet (0 vs 5
  expected), and invalid UTF-8 cannot be checked against a prior source view.
  Pure record preparation is extracted; guarded writes are being added before
  scanner activation.

- The existing empty-code counter fixture is updated because accepted empty
  code now counts as ingested (snapshot), while empty non-code remains skipped.
  Repository identity nodes stay repository-owned rather than file-generation
  contributions, preserving their shared non-code owner and existing identity.

- 2026-10-14: Source-byte reads now run in a database transaction across store
  instances. Activation verifies semantic UTF-8 slices and explicit rendering
  separators against reconstructed snapshots. Generic puts are fenced by an
  immediate transaction; generic mutations invalidate active file views, and
  future minimum source readers are refused before migration.
- T5: full ingest regression suite is green; five source fixtures cover exact
  polyglot/empty source, invalid encoding, failure/retry, fingerprint changes and
  independent full scopes. Source inventory IDs are normalized. Versioned
  sidecars bind scope/fingerprint; stateless native scans use durable source
  hashes as hints. Repository identity hashing moved unchanged into engram-code.
- T6: authorized ImportData code bundles are implemented through the existing
  export/migration services. Legacy methods reject bundles; authorized apply
  fingerprints full content/principal/scope/target and rechecks authorization.
  Exact scope/policy preservation only; translations are explicitly unsupported.
  Three transfer fixtures exercise real target activation, failed snapshot
  import/retry and repository-owned context without file lifecycle ownership.
- T7: bound native source handles, typed Node transport, Rust MCP source
  operations and HTTP source-context launch configuration are wired. Source read
  returns exact text plus coverage. Reader 0 chunk projection remains available;
  unsupported source readers fail closed. HTTP reports source Unsupported when
  trusted source context is absent. Rebuilt native smoke proves exact read and
  second-scan unchanged behavior; Rust MCP unit suite passed 171 tests.
- T4 security refinement: active-generation views alone do not authorize a
  requester. A negative test reproduced private-symbol leakage through graph
  candidates. RecordAccess now filters source-owned keys using current owner,
  exact scope, retrieval use, expiry and private ownership after cache retrieval
  and before ranking. SQLite failures propagate, never become an empty allow
  set. Cached graphs remain principal-independent supersets; labels are reused
  only for unchanged node/edge sets. Shared identical records may be shared by
  any independent authorized owner; candidate policy is a whole authorized
  owner's policy, preferring private visibility, rather than a synthetic merge.
  This preserves retention/deletion semantics and does not grant model use.
- Initial rebuilt Rust MCP live fixture: four code files ingested, zero errors,
  exact TS BOM/CRLF source read, four unchanged on second scan, checkout found
  through four retrieval lanes. Final live re-run remains required after fixes.

### Failure-path completion

Construction test: an explicit source policy change followed by invalid UTF-8
must leave the old source unavailable with the requested policy; correcting the
bytes must converge on an active replacement. The test failed with the old
workspace policy still active, then passed after separating policy exclusion
from byte admission. Legacy reserved ownership remains restricted but cannot
block unrelated retrieval; a missing real-attempt manifest still fails closed.

### Review-driven file-read confinement refinement

Security review identified that canonicalization followed by pathname read could
follow a swapped symlink and allocate a grown file before the size check. T5
therefore uses an ingest-owned directory capability opened once from the trusted
canonical root, and reads relative paths through it with a max-bytes-plus-one
stream bound. Use `cap-std` 4.0.3 in this filesystem adapter only; core and storage
ports remain unchanged. Its root-confined open handles ancestor symlink races
across supported platforms, avoiding a new unsafe/custom syscall implementation.
Contract oracle: https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html and
https://github.com/bytecodealliance/cap-std .

Construction tests before implementation: after walk-time canonicalization,
replace a file with an outside symlink and require read denial; replace its
parent directory with an outside symlink and require read denial; grow a file
past its limit and return only limit+1 bytes for the existing oversize outcome.
A normal BOM/CRLF file must read exactly. These refine existing admission
security/resource controls without changing default limits or parser behavior.
Tempted to implement platform-specific openat2/Windows syscalls directly;
declining because the filesystem adapter can use a reviewed portable capability
library instead of introducing a second unsafe boundary.

Bundled fix (T7 legacy chunk transport): the real Node transfer test discovered
that the SQLite `KnowledgeQuery::list_chunks_by_document` implementation
recursed into itself because the underlying repository trait was not in scope.
The debugger showed 26k repeated async frames and SIGSEGV. Explicit
`KnowledgeRepository` dispatch restores the existing document-chunk API and
lets the actual legacy projection fixture verify snapshot exclusion.

Import refinement: every bundle now rechecks the source-export grant and the
source policy/owner, in addition to target write/activation authority, during
dry-run and apply. A private input with target grants alone cannot activate.

### Final integrated verification

The rebuilt real-addon round-trip (including legacy document chunk listing)
passes after explicit repository trait dispatch. The built HTTP CLI with a
trusted context file returns exact source; client scope overrides are denied.
The root-capability reader's final/ancestor symlink swap and bounded-growth tests
pass on Linux; oversize replacement retains the prior active generation.
`cargo test --workspace` passed 1,054 tests; subsequent focused new regressions,
workspace check/Clippy, generated contracts, TypeScript typecheck/test/build and
contract/docs/neutrality/parity gates passed. The dedicated live MCP smoke uses
fresh and reopened temporary stores; vectors were disabled, with no model call.
The final full TS test run passed, including the CC backend runner that had a
transient RPC timeout during the earlier concurrent debug build.

Security scan scope: a fresh RustSec database audit reports 16 pre-existing
advisories on both lockfiles and no added tuple. This is recorded in the backlog
rather than represented as a clean dependency audit. Role-specific Spark models
were unavailable in this account; independent read-only Luna reviewers performed
the adversarial, quality and security passes using the scoped checklists.

### Final review portability and quality refinement

Normalize native relative path components to `/` before reader/manifest identity;
Windows legacy attribution normalizes its old separators, while literal Unix
backslash names stay distinct. A portable nested-file scan/read fixture is added
to the Windows CI job; local execution is Linux and Windows CI results are not
claimed here. This enforces the already approved portable path contract without
changing repository keys or symbol identity rules.

Preserve the parser attempt outcome rather than discarding errors with `.ok()`:
no applicable grammar is Fallback; parser/setup failure is Failed; valid/error
ASTs remain Clean/Recovered. A pure four-outcome construction fixture exercises
an injected setup error independently of exact source retention. No parser or
language registry was added. The spec index now matches delivered status.

Bundled CI gate repair: vector feature checks still named the retired
`engram-store-vector` crate. Both workflows now check the accepted SQLite adapter
cell's existing `fastembed-tests` feature; no provider/model invocation is added.
The Windows source job is a build/test gate, not a deployment.
