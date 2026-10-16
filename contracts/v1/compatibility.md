# v1 Compatibility Rules

After v1 acceptance, compatible changes are limited to:

- Add an optional field.
- Add a new schema file for a new operation or payload.
- Add a new enum value only when consumers are explicitly required to tolerate
  unknown values for that enum.
- Add examples or specs that do not change existing field meaning.
- Deprecate a field without removing it.
- Tighten explanatory text without changing behavior.

Breaking changes require v2:

- Rename a field.
- Remove a field.
- Change field meaning.
- Change enum value meaning.
- Make an optional field required.
- Make a nullable/omittable field non-nullable or non-omittable.
- Change identifier semantics.
- Change required policy or provenance behavior.
- Add storage-specific, provider-specific, language-specific, or gateway-specific
  concepts to the portable contract.

V1 consumers must treat identifiers as opaque strings. They must not parse
tenant, workspace, timestamps, or storage locations from IDs.

`metadata` may carry arbitrary JSON-compatible values, but no v1 behavior may
depend on metadata keys unless a later version promotes those keys to typed
fields.

## Enum extensions applied

- **RetrievalTargetType** (RFC-0013 Phase 1, 2026-07-13): added `rule`,
  `policy`, `axiom`, `decision_trace`. Consumers must tolerate unknown
  `RetrievalTargetType` values per the enum-add rule above; exhaustive `match`
  sites require a wildcard arm or updated arms.

## Code source preservation (RFC-0021)

KnowledgeChunk gains optional typed code/documentKind fields. New source
record payloads use the version-1 code-source contract and the existing
`ImportData` envelope. Existing chunk-kind enum values and `SourceLocation`
offsets are unchanged; new descriptor ranges are zero-based, end-exclusive UTF-8
byte offsets. JSON readers that tolerate optional extensions can continue to
read records, while strict old validators need legacy projections. Adding
optional fields to public Rust structs can still break downstream struct
literals, which must be updated. Generated TypeScript types follow the accepted
schema.

Code-source reads are distinct from recall and take only a repository/path
query. A trusted host supplies `CodeSourceContext` (exact scope, requester, and
reader version); request fields cannot widen that context. The HTTP MCP reads
this context from the file passed to `--source-context`; without it, source
operations report Unsupported. Rust MCP uses its launch-bound scope and
requester and has no separate `--source-context` option. Private sources are
readable by their recorded owner; another requester needs `code:source:read`
and must still pass source policy. Export needs `code:source:export`; import
activation needs `code:source:write` and `code:source:activate`. These grants
are not interchangeable, and reading source does not authorize model egress.

Source bundles preserve exact source scope and policy. Scope translation is
unsupported in this version. Legacy rows have unknown source coverage. Readers
that do not understand source roles must not be used to activate snapshot
writers; source-capable backends are reported through capability conformance,
and unsupported source cells fail as Unsupported rather than returning an
empty successful read. Check the active provider's capability report before
relying on source operations.

The Node crate's public Rust name-match helper `resolve_cross_file_edges` now
returns `CoreResult<()>` so hosts can report failures. It excludes file-owned
code entities; deterministic typed cross-file code resolution remains in ingest.
Rust callers must handle its result (source-level API change).
