# Spec: Code source preservation

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** [RFC-0021](../../rfcs/0021-structural-code-context.md), [ADR-0029](../../adr/0029-normalized-code-parser-port.md), [ADR-0030](../../adr/0030-code-derivation-layers.md), [ADR-0022](../../adr/0022-engine-grid-vs-backend-recipe.md), [ADR-0028](../../adr/0028-engram-code-crate.md) (remaining ownership rules)
- **Brief:** none
- **Contract:** source-preservation extensions to [domain model](../../domain-data-model.md), [accepted wire schema](../../../contracts/v1/schemas/engram-v1.schema.json) and [ImportData](../../../core/integration/src/migration.rs), authored first in plan T0; existing scan/read operations are reused. These extensions are implemented by this delivery slice.
- **Shape:** mixed

> **Spec contract:** this document defines what “done” means. The implementation
> matches this contract or updates it in the same change. Draft status means
> unchecked criteria are required work, not shipped capabilities.

## Objective

Agents and downstream explainers obtain complete source for every accepted
UTF-8 code file, complete callable bodies even with nested declarations, and
recoverable field/initializer/import context. Snapshots reproduce exact bytes;
semantic chunks describe their actual source coverage. Normal retrieval and
embedding do not treat snapshot-only records as semantic candidates. Re-scans
expose only a complete active file generation or an explicit unavailable state,
with source/version policy enforced on Rust and TypeScript transports.

This is the first RFC-0021 delivery slice. It provides source authority and
coverage; it does not generate summaries, add parsers, implement full context
hydration or solve tokenizer segmentation.

## Assumptions

- Code extraction is deterministic behavior in `engram-code`; ingest owns filesystem/git orchestration and knowledge adapters persist records. (`core/code/src/lib.rs`, `adapters/ingest/src/scanner/mod.rs`, ADR-0029)
- Existing `ChunkRef` reports text presence/hash but not role/generation; snapshot exclusion needs an explicit selection contract. (`core/integration/src/knowledge_query.rs`)
- The default file limit is 1 MiB, and existing path/secret/ignore/minified controls remain in force. (`adapters/ingest/src/scanner/mod.rs`)
- The repository stable source key is shared by its files; file-view identity includes the normalized relative path. (`scan_repository` source_key construction, `reconcile::delete_prior_graphs_for_path`)
- Source preservation precedes parser/context, segmentation/retrieval and Silver delivery. (User approval of RFC-0021 and delivery sequence, 2026-10-14)
- Domain semantics precede wire schema/Rust/TS projections; Draft/Implementing/Shipped statuses track spec delivery. (`AGENTS.md`, `docs/CONVENTIONS.md`)

## Boundaries

### Always do

- Preserve accepted source exactly, including whitespace, comment markers,
  Unicode, BOM and CRLF; report unsupported encoding before destructive writes.
- Preserve complete callable bodies and signatures. Separate container context
  from its declaration span; label extraction/fallback coverage honestly.
- Keep typed code roles/ranges/generation outside arbitrary metadata. Update
  the domain model, then accepted wire schemas and generated projections.
- Apply policy/scope to snapshot reads, generation operations, retraction and
  candidate eligibility. Reuse existing knowledge and retrieval ports.
- Deliver reader/index selection before enabling snapshot writers. Keep
  all supported transports in parity and capability reports truthful.
- Keep the current security filters and file-size default; source denied by
  the scanner does not enter snapshot storage.

### Ask first

- Adding a grammar/provider, model invocation or new process/network integration.
- Changing logical entity identity, repository stable-key semantics, memory or
  belief contracts, or the existing retention/deletion meaning.
- Increasing default scan resource limits or exporting source to a new destination.

These are changes outside this approved slice; ordinary implementation choices
within the accepted RFC/spec do not require repeated permission.

### Never do

- Reduce an executable outer function to its signature because it has nested declarations.
- Claim a reconstructed complete snapshot proves complete semantic extraction.
- Embed/index raw snapshots through any normal candidate feed, including dedup reuse.
- Introduce code-specific persistence into a memory/vector adapter, core SQL,
  a new core facade or a monolithic scanner/generation manager.
- Treat missing generation/role on legacy chunks as a complete new source view.
- Advertise cross-store ACID, undo already-succeeded writes as rollback, or
  hide a file write/cleanup failure by stamping it unchanged.

## Behavioral contract

### Accepted source and snapshots

A file is accepted only after existing scanner admission checks, byte-size
validation and strict UTF-8 decoding. Invalid UTF-8 is a typed unsupported
encoding outcome; no replacement characters are silently substituted. Read or
admission failure leaves the prior generation unchanged unless an explicit
policy/retraction operation requires exclusion. Empty and whitespace-only
accepted code files have a snapshot and zero semantic units; an empty file is
not an ingestion failure. Filtered/generated/secret/oversize files remain
explicitly skipped and are not claimed as snapshot-covered.

Snapshot storage uses existing knowledge documents and chunks. Each snapshot
chunk has role `source_snapshot`, ordered byte range, file-view identity,
content digest and generation. Ranges form a contiguous, non-overlapping
partition of the original UTF-8 bytes; concatenation hashes to the original
file digest. A zero-byte file has a single `[0,0)` snapshot range. Raw snapshot
text has no synthesized headings/comments and is not summarized or clipped.
Storage segmentation does not assert semantic boundaries.

New typed ranges are zero-based, end-exclusive UTF-8 byte offsets. New code
ranges do not redefine legacy `SourceLocation` offsets; line coordinates are
an additional convenience. Composite container context records identify each
ordered source slice and render separators separately. Declaration span,
body/signature spans and actual text coverage are distinct concepts.

### Semantic source coverage

Supported mapped callables keep complete bodies and full multiline signatures,
including statements before/after nested declarations. Nested units retain
qualified anchors and their own source slices. Bounded duplication is allowed;
per-unit identity and graph naming remain governed by existing code contracts.

Structural containers retain their complete header and relevant field /
initializer declarations, with exact source slices recoverable from snapshots.
Imports, annotations/decorators, anonymous syntax and top-level executable code
outside mapped units are preserved as residual context. Unmapped features have
explicit unsupported/partial coverage. Clean/recovered/fallback/failed parse
status is independent of snapshot completeness; recovery trees are not silently
reported clean. Fallback is deterministic and labeled, and its accepted source
still has an exact snapshot. No new grammar is needed to satisfy this slice.

### Typed descriptors and eligibility

The optional code descriptor links existing `KnowledgeChunk` records to their
role, file-view key, generation, source digest, parser/mapping/chunk-policy
fingerprint, owning unit, actual ranges and completeness. This slice's writers
emit snapshot, symbol, container_context and residual roles. Future summary /
segment roles belong to their own specs; they are not advertised as shipped.

The same eligibility rule applies to default recall, code search, lexical,
vector, associative/community feeds and context composition: snapshot-only
records are not candidates, and code items from inactive/mismatched file
views are excluded. Snapshot lookup is an explicit source-read purpose with
its own scope/policy checks. Public raw record access can expose a snapshot
authorized for that requester; it cannot turn that record into a recall hit.
Extraction routines that enumerate document chunks for a model are also
role-aware and do not automatically feed the raw snapshot copy.

Explicit source read and normal retrieval do not authorize external model egress.
Any code-source role, including legacy code with unknown coverage, requires
RFC-0021's separate per-source enrichment/destination authorization before
entering a model-document feed. Until that authorization contract is available,
automatic external model feeds exclude code sources entirely; existing non-code
document processing is unchanged. The subsequent
[local-code-embeddings](../local-code-embeddings/spec.md) slice permits current,
authorized semantic code chunks through an explicitly in-process embedder.
Snapshots and legacy-unknown code remain excluded. This source-preservation
slice creates no external model calls or egress grants.

Source reads resolve an effective scope from trusted host/requester context
and use exact tuple matching plus policy authorization. Omitted request fields
cannot broaden subject/workspace/session/environment access: they are resolved
from trusted context or denied if unresolved. A resolved null dimension matches
only null, never every value. The new source-read operation does not inherit
wildcard behavior from a partially specified generic query scope. Existing
ordinary retrieval scope semantics are unchanged by this slice.

Legacy records without the descriptor retain existing read behavior and are
labeled `legacy_unknown` where coverage is requested. A migrated file's legacy
chunks/index entries cannot compete with its active new generation. Missing
new backend support is reported Unsupported; it cannot return an empty result
as if a source snapshot had been read successfully.

### File generations and recovery

A file-view key is `(scope, repository stable source key, normalized relative
path)`. The repository key remains unchanged; it is not sufficient alone to
identify a file generation. This is the per-source activation identity used
by RFC-0021, realized here without hiding unrelated files in that repository.
Generation identity also binds source digest and extraction fingerprint.

The authoritative manifest lives behind a knowledge capability port with
atomic compare-and-set behavior in the knowledge adapter. State is active,
updating or incomplete, with an expected generation and coverage inventory.
A removal/policy retraction first excludes the current contribution and retains
an incomplete cleanup marker until durable cleanup succeeds. Shared graph
facts remain eligible for an independently active source contribution only.

Before destructive replacement, the scanner obtains a generation lease and
marks that file updating. It persists snapshot/unit/graph writes, verifies
coverage/inventory, then compare-and-sets active. In-flight reads filter each
candidate contribution and recheck selected file generations before response.
Updating/incomplete sources yield explicit source failures; unrelated code /
memory lanes continue. Older scan attempts cannot activate after supersession.

Failures after replacement begins leave the file incomplete. Retrying uses
content/version-bound IDs and the expected generation; required cleanup or
writes are retried idempotently. Successful snapshot/chunk/graph writes are not
rolled back. Derived vectors can be pending when a source activates, but old /
wrong-generation vectors are ineligible. Snapshot deletion follows existing
source policy and children-first knowledge retraction; no source artifact
remains readable after its policy denies access.

Historical reads return only retained matching snapshot/generation evidence.
If retraction removes historical data, the requested old view is unavailable;
the current working tree is not a substitute. This slice promises no new
historical-retention policy.

### Migration, export and public compatibility

Extraction fingerprints include parser/grammar version, mapping version and
chunk-policy version. Changed fingerprints requeue affected unchanged files;
old host manifests do not authorize skipping a required re-scan. Current
writes use a negotiated code-source contract; strict old readers retain their
existing projected responses. The store records the minimum role-aware reader
version, and incompatible old writers/readers cannot silently reopen it.

Optional wire fields are additive only for consumers accepting them. Rust
struct literal changes and strict old-schema validation need explicit caller /
projection handling. Existing chunk-kind values and legacy offset meanings
are unchanged. Role/generation descriptors are typed, not metadata-only escape
hatches. Acceptance requires the exact domain/schema diff and compatibility
classification at implementation T0, before any writer starts.

Export/import uses existing `ImportData` and `MigrationService` with typed,
versioned extensions for descriptors and file-generation state. Snapshot bytes,
policy/provenance and matching source state round-trip; loss is an error or an
explicit unsupported mapping. There is no new export format or duplicate SQL
export implementation. Import to a differently scoped target validates and
rewrites file-view scope consistently, denies widening policy, and activates
only after the target coverage/inventory verifies.
Target writes/activation require explicit authorization in the fully resolved
target scope and source-export authorization for every input. Default import
preserves the exact source scope/policy. Cross-scope translation additionally
requires an explicit approved source-to-target mapping; it cannot broaden
visibility, allowed uses or processing destinations, extend permitted lifetime,
or discard subject/tenant/session restrictions. Incomparable/unrepresentable
restrictions are rejected before activation. Imported snapshots retain their
role and exclusions on every normal index/retrieval/model-document lane.

## Testing Strategy

| Outcome | Mode and surface | Why |
| --- | --- | --- |
| Exact source, complete callables and context ranges | TDD, pure unit/property tests in `engram-code` | Byte coverage and boundaries are compact deterministic invariants. |
| Role-aware feeds and legacy projection | TDD, retrieval/transport contract fixtures | A negative snapshot hit is a meaningful cross-lane behavior, not a mock call assertion. |
| Generation activation/failure/retry | TDD, knowledge adapter and scanner integration | State transitions and injected write failures prove BestEffort honesty. |
| Portable records/export/backend parity | Goal-based generated-contract/type checks plus recipe conformance | Portable shapes must remain reproducible and persist identically. |
| Real source scan/read/re-scan and search exclusion | Live stdio MCP and Rust/N-API flow | Unit tests alone cannot prove actual host/store wiring. |

## Acceptance Criteria

- [x] **AC1 — Exact snapshots:** accepted TS/Rust/Python, import-only, malformed and unknown-parser code fixtures reconstruct exactly, including Unicode/CRLF/BOM, empty and whitespace-only input; invalid UTF-8/filtered/oversize files are reported without false coverage.
- [x] **AC2 — Complete callable bodies:** `checkout` retains fee assignment, validation error and return around its nested function; multiline method signatures/bodies and receiver-qualified nested anchors are preserved across TS/Rust/Python.
- [x] **AC3 — Recoverable context:** `UserService`'s field, constructor, imports, both methods and unmapped residual syntax have exact recoverable ranges; container context never reports its abbreviated text as the whole declaration.
- [x] **AC4 — Honest coverage:** clean/recovered/fallback/failed quality and per-feature support are visible separately from full snapshot retention; legacy source completeness is unknown.
- [x] **AC5 — Snapshot exclusion:** normal retrieval/index/model-document feeds never use snapshot-only records, including content-hash embedding reuse and after export/import; explicit authorized snapshot reads succeed, omitted/broad/wrong-scope requests fail closed, and no source read implies model egress authorization.
- [x] **AC6 — Per-file activation:** one file changing leaves sibling file views available; partial writes or superseded attempts cannot expose/activate a mixed generation on source, graph, lexical or vector lanes; shared-source contributions remain independently scoped.
- [x] **AC7 — Recovery/retraction:** injected snapshot/unit/graph/cleanup failures leave incomplete status and retryable manifests; retries converge without duplicates or cross-store rollback claims; deletions/policy restriction remove visibility immediately and cleanup completes idempotently.
- [x] **AC8 — Versioned rescans:** changed parser/mapping/chunk-policy fingerprints requeue unchanged source; repeated scans with the same bytes/fingerprint converge to unchanged with no duplicate snapshots/units.
- [x] **AC9 — Compatibility:** domain/schema/Rust/generated TS changes are classified, old record projections remain supported, minimum reader protection is enforced, and no existing enum value or offset meaning changes.
- [x] **AC10 — Export/import:** existing services round-trip snapshots/descriptors/generation/policy/provenance with snapshot exclusions intact; unsupported readers/targets report a mapping error; source-export and target-write/activation authorization precede explicit non-widening scope translation, and incompatible policies cannot activate.
- [x] **AC11 — Surface/backend parity:** Rust facade, N-API/TS, Rust MCP and TS HTTP MCP expose equivalent authorized source-read and generation/coverage behavior; each supported recipe either conforms or advertises Unsupported before writer activation.
- [x] **AC12 — Live proof:** fresh and existing temporary stores exercise exact source read, nested callable retrieval, consecutive re-scan, removal and snapshot exclusion; commands/results are recorded, with unsupported/vector-disabled lanes not claimed tested.

## Delivery evidence — 2026-10-14

- `cargo test --workspace`: 1,054 passing tests, zero failures; later focused
  admission/source-coverage and cross-scope resolver regression tests also pass.
  `cargo check --workspace`, workspace Clippy with `-D warnings`, formatting,
  contract generation, TS typecheck/build/test, and contract/docs/neutrality/
  surface-parity gates pass. The final TS test run includes 18 Node and 78 runtime
  tests, exercising the actual rebuilt addon and HTTP CLI.
- Core source coverage and ingest fixtures preserve TS/Rust/Python, BOM/CRLF,
  import-only, malformed and unmapped-parser code, empty/whitespace source.
  SQLite construction tests reject bad partitions/digests/parent units and
  superseded attempts; scanner tests inject snapshot/unit/graph/cleanup failures.
- `python3 scripts/dev/code_source_smoke.py`: actual rebuilt stdio server,
  disposable fresh and reopened stores, four exact source reads, full nested
  callable retrieval, snapshot-free canonical candidates, repeat scan, invalid
  bytes preserving prior source, removal and sibling availability. Scan ingests
  four files with zero errors; repeat scans report four unchanged and zero
  ingested. After removal, the sibling remains readable.
- Real Node transfer round-trips exact bytes, policy/provenance and descriptors
  through existing export/import; source-export and scope denial are checked at
  preview/apply, and actual reader-zero/get/list projections exclude snapshots.
  HTTP MCP and its built `--source-context` CLI return identical source and
  reject tool-supplied scope.
- Live vectors/model calls were disabled and are not claimed exercised. Vector
  resolver eligibility/revision guards compile under the standard fastembed
  build; deterministic adapter tests and shared source-selection tests cover
  their underlying invariants. Symlink race fixtures ran on Linux; Unix tests
  are conditional and Windows runtime verification was not performed here.
- `cargo audit` reports the same 16 advisory/package/version tuples on the
  baseline and this branch, not a green audit gate; see the separate
  [dependency-security baseline](../../backlog.md#dependency-security-baseline).
