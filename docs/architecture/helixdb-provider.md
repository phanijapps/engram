# Helix provider composition

The selected-provider factory in `backends/provider` validates portable configuration
before opening exactly one backend. SQLite remains the default. A legacy PG
connection selects PG when no explicit provider overrides it. Helix requires an
explicit `service` or `embedded` mode and logical store. Missing build features,
conflicting settings, unsafe paths, denied authority and selected-store failures
return errors; they never trigger another-store fallback.

## Owners and host paths

| Responsibility | Canonical owner |
| --- | --- |
| Provider selection, feature checks, path preflight | `backends/provider/src/selection.rs` |
| Canonical embedding preparation before native partition identity | `backends/provider/src/construction.rs`, `core/integration/src/embedding_runtime.rs` |
| Helix recipe and selected handles | `backends/helixdb/src/lib.rs` |
| Native storage/session/transaction/receipt handling | `adapters/helixdb` |
| Governed memory validation, policy, ranking and lifecycle | `core/memory` |
| Batch, provenance, recall, community, diagnostics, archive orchestration | `core/integration` |
| Exact call result and effect identity | `core/runtime/src/effect.rs` |

Rust uses the factory with an explicit `PolicyAuthorizer`. Native/TS constructors,
the retained integration bootstrap, MCP stdio and applicable one-shots delegate to
that factory. The native binding exposes selected ontology/taxonomy mutations as
well as query handles; a handle name alone is not operation conformance proof.
TS-owned temporary proxies close in `finally`. Caller-retained proxies keep their
own lifetime; closing a provider does not invalidate an independently retained
capability. All owned handles must close before final resource release.

Code operations still pass the canonical code coordinator and its authorizer.
The recipe denies code authority by default. Host launch grants are scope grants,
not authentication of a remote caller. No host repeats memory policy, rank fusion,
publication eligibility, reset gates or source-supersession algorithms.

## Scope and storage identity

A partition binds the logical store and full `Scope`: tenant, subject, workspace,
session and environment, together with its embedding-space identity when present.
Typed inventory validates payload identity, physical keys and parent ownership.
Documents and chunks inherit their source's scope; concepts inherit their scheme's
scope. A child cannot claim a different tenant or session by omitting its parent.
MCP resolves `--org` to tenant and domain/subdomain or project to workspace. Preserve
these launch values between writer and recall processes; they do not expose new
subject/session/environment CLI flags.

Embedded storage and derived caches must remain under the validated trusted root.
Preflight checks selection, mode and packaged features before storage/cache opens,
model initialization, credentials or connections. Existing and missing descendants
are confined, including symlink/traversal checks. Read-only one-shots use native
read-only access and a lexical reader without taking the writer lock. Reader reloads
observe committed index state. A missing or unusable lexical cache returns a lane
failure; it is not replaced with a successful empty lane. Other eligible lanes may
still produce a degraded recall with that failure reported.

## Modes and current network restriction

Embedded owns its in-process native runtime. Only the final owned handle can drain
and join it before releasing writer ownership. Service close releases this client's
resources and never terminates the external server. A competing writer and uncertain
closure are errors; do not assume a replacement writer is safe after a failed close.

**Effective PLT-001 remains strict:** vectors disabled means no model initialization,
model credentials or HTTP calls. Service with vectors off therefore rejects before
credentials/network. Until the final ratified application is implemented and proved,
service positives explicitly set `enable_vector=true` and embedding provider `none`.
That allows the configured storage service without initializing inference. The
ratified future storage/auth exception is not active merely because the provider is
packaged. Embedded with vectors off remains an offline storage path.

Storage features are independent of embedding features. A healthy configured
embedder is prepared once before opening the partition; its actual resolved
semantic identity and dimensions are used by the selected recipe. An explicit
incompatible space fails rather than disabling healthy inference or changing stores.
Unavailable local-model degradation does not prove active embedding coverage.

## Archive v1 boundary

`ArchiveExport` consumes the typed scope-bounded inventory. The unchanged v1 export
contains memories, sources, documents, chunks, entities, relationships, concept
schemes, concepts, and available beliefs/hierarchy nodes. Optional family inventories
can be empty when the recipe has no corresponding store. This is a projection,
not a lossless backup or a new restore format:

- Vectors are omitted. Memory export retains text, creation timestamp and serialized
  policy, but does not preserve every lifecycle/kind/provenance field.
- Document content is empty; text is in chunks. Chunk sequence is the legacy zero
  value, rather than an invented source ordering.
- Relationship evidence/confidence/provenance and richer vocabulary structures do
  not fit their v1 records. Scheme description and concept metadata retain legacy
  absent/empty representations.
- Events, code publication/binding/reset journals, auxiliary ledgers and derived
  indexes are not archive-v1 operation-family backups.
- Belief validity start/end is projected, with creation time as the absent-start
  fallback; a belief's full record-time history is not exported.

Every exported scope string preserves the complete tuple. Parent-owned documents,
chunks and concepts use the validated inherited scope. Dry-run import checks the
existing key-field fingerprint, row counts, basic scope translation and vector
dimensions. It writes nothing and is not full semantic validation of every field.
Apply always returns explicit unsupported before writes, including valid, stale or
invalid manifests. Do not advertise successful dry-run as restore support.

## Uncertainty and recovery

The adapter generates the real commit UUID before possible dispatch and binds it to
the exact call/session/item/step frame. Acknowledged, definite failure, unknown and
blocked-by-prior-unknown are distinct typed outcomes; cleanup certainty is separate.
Public errors keep their existing envelopes and redact diagnostic content while
preserving the original opaque UUID. Neither text parsing nor a global last error
establishes causal identity.

On unknown outcome, stop original logical work. Preserve the original receipt,
partition, owner/epoch and invocation-owned data. Earlier acknowledged BestEffort
steps remain durable; later unexecuted steps stay skipped. Never replay a whole
batch, invent rollback, retry the original mutation blindly or create a new UUID to
hide uncertainty. A blocked admission refers to the prior UUID and performs no new
send. Reconciliation reads and validates the exact durable receipt before confirming
owned cleanup; a successful read elsewhere is insufficient. Definite failure may
permit ordinary BestEffort continuation, but does not erase an earlier unknown.

Instrumented fixtures are a separate nondefault `test-controls` artifact. Native
private initialization and MCP `__fixture` attach actual transport controls before
alias composition. A recorder-supplied source/artifact hash is not attestation that
those bytes executed. Bind consumed artifact hashes independently. Production
builds omit fixture controls; no product environment flag enables them.

See [provider builds](../development/provider-builds.md) for inventory and fixture
protocols, and [launcher configuration](../../packages/launcher/CONFIGURATION.md)
for the operator flags. Build inventory, simulated harness controls and passing
constructors do not establish the complete native workflow or network matrix.
