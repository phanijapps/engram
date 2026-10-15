# Plan: Reliable natural-language code intelligence

- **Spec:** [spec.md](spec.md)
- **Status:** Executing

## Approach

Freeze the evaluation first, promote the reviewed portable contracts, then build
configuration and current-document authority. Add derivative AST/token coverage
without replacing canonical methods. Separate candidate retrieval from output,
wire a real local scorer, hydrate evidence and expose one operation through all
transports. Calibrate on development data and run untouched held-out/live-store
gates. A failed quality gate blocks shipping; it is not resolved by relabeling
queries or claiming that any vector hit is sufficient.

## Constraints

Follow [reference architecture](../../architecture/reference.md): Behavior
crates own deterministic algorithms/ports; Adapter crates own persistence/model
lifecycle; Binding/Package layers transport generated Rust-owned contracts.
ADR-0009 owns composition, ADR-0022 neutrality/parity, ADR-0026 fusion configuration,
ADR-0028/0029 code/parser boundaries, ADR-0030 derived knowledge. Use
[research](../../research/reliable-code-intelligence-survey.md) and
[frozen original evaluation](../../research/code-intelligence-self-evaluation-2026-10-14.json)
as evidence, not alternative behavior contracts.

## Declined patterns

Reuse existing fusion and source authorization rather than adding alternate
scoring services or authorization caches. Keep model/tokenizer integrations in
adapters; do not introduce remote summaries, a service locator, or core env reads.

## Construction tests

All task tests map to spec ACs. Cross-cutting tests run the same minimal/overridden
configuration and corpus through Rust/native/stdio/HTTP, including unauthorized
and stale candidates, model fallback and actual scored results. Network
instrumentation proves that normal model inference performs no asset download
or source upload. Model-backed tests use explicitly provisioned, pinned assets;
missing-cache tests run with networking unavailable. Legacy public-contract and
memory/recall fixtures remain the regression boundary.

## Design (LLD)

### Data and representation

Promote reviewed definitions from the [draft schema](../../../contracts/jsonschema/reliable-code-intelligence.draft.schema.json)
through `docs/domain-data-model.md`, accepted `contracts/v1/` and reproducible
generation. Configuration overlays are distinct from complete effective config;
JSON Schema defaults are annotations, not a merge implementation.

Keep canonical `CodeChunkRole`/source-inventory meanings unchanged.
`CodeEmbeddingSegment` is a separate knowledge-owned derived record with typed
parent, ranges, source generation/attempt and representation ID. A neutral port
resolves segment IDs to canonical parents; vector resolvers use that port before
existing source authorization. No synthetic method fragment is called a complete
canonical symbol, and no new role enum is required merely to store segments.
Retraction and failed activation invalidate segment ownership through their
canonical parent. Hash reuse includes compatible representation identity.

`DocumentHead` has a knowledge-owned port with exact source identity, CAS and
inventory-verified activation. Secondary indexes consult that authority. Legacy
head adoption requires fresh verified input; scanner's unchanged path recognizes
missing authority and schedules verification rather than skipping it forever.
Failed source/import attempts keep the head unavailable without rolling back
already written canonical records. Migration is additive and reversible by
selecting existing retrieval behavior; canonical/history data is never wiped.

### Decomposition and interfaces

- `core/retrieval`: reuse fusion/composer/config validation; portable candidate
  budgets and stable parent aggregation, without SQL/model/environment I/O.
- `core/code`: pure normalized structural range segmentation; injected token
  boundary/count data keeps tokenizer/model integrations outside pure code.
- `core/knowledge`: segment/document-head ports and source-grounded validation.
- `core/integration`: thin provider operation/config API; focused modules for
  code-evidence orchestration, eligibility, context selection and diagnostics.
  Each stage consumes existing ports; no duplicated monolithic recall pipeline.
- Knowledge adapter cells: head/segment persistence and canonical resolution.
  Model adapter: local ONNX assets/tokenization/batched scorer under a feature.
- Recipes compose handles; `bindings/node`, packages/runtime/native and MCP
  wrappers call the shared operation. Supported backend recipes implement or
  explicitly report missing new ports; no transport-only capability is shipped.

Candidate selection scores segments, aggregates by canonical parent, caps output
and resolves supporting source. Disabled lanes are not called. Context and
supporting facts have independent ownership checks; cached source maps are
request-bound and source-revision checked, never reusable authorization grants.
A bounded final recheck drops changed units or retries once; unstable sources
produce explicit insufficiency rather than an unbounded retry loop.

The scorer pool selects one best segment per parent, then round-robin additional
segments. Only scored parents enter successful cross-encoder ordering; none/
fallback uses full bounded RRF with partial model scores discarded. Code profiles
reject MMR/other unsupported strategies without changing generic recall.
Normalize source categories per unit, including test attributes/ancestry in mixed
files. Nonimplementation/unknown units are supporting evidence only.

### Scorer construction and failures

Use installed FastEmbed 5.17.2's `TextRerank` / `JINARerankerV1TurboEn` behind
`RerankScorer`; inspect the real API before integration. Add a backwards-compatible
batch-scoring default method to the scorer trait if necessary; existing single-pair
implementations still work. The local override remaps sorted model indices to
input order and validates uniqueness, count and finite outputs. Stable reranking
retains provenance/policy and actual scorer traces.

Add a backwards-compatible locality declaration defaulting to nonlocal; code
evidence rejects unknown/external scorer implementations before input processing,
even if an SDK caller injects one under an admitted model label. Deterministic
local stubs explicitly opt in; actual-model gates still require ONNX inference.

A model-registry manifest pins the official ONNX/tokenizer assets, revision,
hashes, Apache-2.0 license, token/window and score transformation. It is local
adapter configuration, not a core model dependency. Provisioning reads only
allowlisted artifact sources; arbitrary URL/env model loading and remote Python
code are absent. Actual artifact compatibility and code-ranking quality are
construction gates, not inferred from the model card.

Missing/corrupt/failed scorer uses the spec's degrade/error policy. No silent
cross-encoder fallback, fabricated scores, unbounded batches or hard-cancellation
promise. Calibration belongs to a model/query/representation/config identity;
overrides that invalidate it report unvalidated status. Fast mode avoids scorer
loading/calls and shares the same index representation.

Unvalidated calibration or effective-strategy fallback always returns insufficient
status, even if authorized inspection candidates remain. Thresholds use the
recorded strategy score space after parent aggregation and before output caps.
Fast needs separate calibration. Structured excerpts, segment traces/token
coverage, context and unresolved facts follow the draft packet definitions and
256 KiB serialized limit.

## Tasks

### T0: Freeze corpora, labels and reference machine

**Execution:** Complete: frozen inputs, independent label/harness review,
original12+identifier2+three-corpus45 development requests and four live
authorization controls. [Raw baseline](../../research/reliable-code-intelligence-baseline-2026-10-14.json).
[Independent review](reviews/t0-2026-10-14.md). Candidate/phase metrics unavailable
in legacy search remain unavailable. T10 verifies these results before tuning.

**Depends on:** none
**Mode:** goal-based check plus recorded blind label review.
**Spec mapping:** AC10–AC12.
**Tests:** Manifest has immutable corpus commits/hashes, hardware/runtime settings,
query order, expected path/qualified-unit/range labels and metric definitions;
labels and queries are outside every indexed corpus. Original 10+2 probes and
identifier controls are reproduced before tuning. At least 40 answerable and
20 negative queries are held out, with >=10 answerable queries per repo, across
Engram plus two structurally different TypeScript/Python repos. A separate
>=30 answerable/15 negative development set supports tuning/calibration. Negative
labels distinguish absent feature, irrelevant question and authorization denial.
**Approach:** Add `core/eval` fixtures and a real-MCP evaluation runner with
sequential response timing; capture per-lane candidate recall, ranking, evidence
coverage and baseline phase costs. Freeze local CPU machine/runtime before any
optimization. No expected-answer artifacts enter scan inputs.
**Done when:** Corpus/label/query-order hashes and the baseline executable are
frozen, independent blind label review and runner construction gates pass, and
serialized baseline collection is running against those immutable inputs.
Completed baseline results (original controls and all three development corpora)
remain a hard prerequisite of T10; collect them before any tuning or held-out
scored evaluation. This separates measurement duration from contract work without
changing a quality target or the frozen corpus. Held-out expected labels remain
inaccessible to tuning/scoring code.

### T1: Promote reviewed contracts and defaults

**Execution:** Complete: accepted additive schema, generated TS, strict portable
DTOs and profile defaults pass focused tests. Independent adversarial/security
and quality fallback reviews are clean. Model/transport behavior is not inferred
from this checkpoint. See `reviews/t1-2026-10-14.md`.

**Depends on:** T0
**Mode:** TDD plus goal-based contract generation.
**Spec mapping:** AC1, AC7–AC8.
**Tests:** Draft examples and invalid cases validate; portable DTO round-trips,
profile defaults, query bounds and semantic cross-field checks match schemas;
existing v1 payloads/types remain compatible. Query/repository UTF-8 byte
admission rejects oversized input before any tokenizer/scorer call.
**Construction:** `stub: true` — `core/domain/tests/code_evidence.rs` and
`core/retrieval/tests/code_config.rs` were red on missing APIs, then passed for
byte admission, opaque source references, profile defaults, strict effective
fusion and token-window inequalities. Model-call and transport assertions are
completed by T8/T9 rather than inferred from these pure DTO checks.
Complete balanced/fast examples and a two-segment scored packet exercise all
diagnostics, accepted failure references and source-range mapping.
**Approach:** After spec approval, finalize the draft domain extension, schema
references, model-manifest/calibration descriptors and accepted v1 artifacts.
Generate Rust/TS surfaces through the established pipeline. Record source-level
updates for explicit struct literals/custom transports. Do not hand-edit generated
outputs or promote a draft merely to satisfy a linker/lint.
**Done when:** Contracts/defaults are reproducible, classified compatible and
reviewed; no newly accepted enum silently breaks legacy source-role semantics.

### T2: Shared config resolution and host loading

**Execution:** Complete: immutable host snapshots, strict presence-preserving
file/env/explicit overlays and allowlisted query reductions pass focused tests.
The real native addon returns the shared snapshot; workspace typecheck/build
pass. Independent secure-design and quality fallback reviews are clean.
See `reviews/t2-2026-10-14.md`. Real model readiness and evidence transports
remain T7–T9 acceptance work.

**Depends on:** T1
**Mode:** TDD and transport integration.
**Spec mapping:** AC1, AC8.
**Tests:** File/env/explicit/profile precedence, omitted vs falsy fields, profile
selection before expansion, bad suffixes/unknown keys, legacy-file isolation,
model-dependent budgets and cross-surface effective hashes. Scanned repo config
cannot become trusted launch configuration.
Unsupported strategies fail before initialization. Independent graph lanes versus
anchored expansion and model/token/query inequalities have explicit tests.
**Approach:** Extend `core/integration/src/config.rs` using focused modules and
existing serde loading; hosts collect explicit file/env inputs. Add MCP config
path selection and native/HTTP host adapters. Reuse fusion validation rather
than creating conflicting strategy/weight fields.
**Done when:** Rust and TS launch paths resolve the same snapshot and origin map.

### T3: Current document authority and legacy adoption

**Execution:** Implementation and focused construction gates complete; independent
secure-design and quality fallback re-reviews clean. Real vector-enabled
stdio verification preserves four indexed versions byte-for-byte while returning
only the current code, and excludes a deleted Markdown file on a force scan without
a host manifest. Retained self-store adoption remains the integrated T11 gate.
See `reviews/t3-2026-10-14.md` and the recorded live artifact.

**Depends on:** T1
**Mode:** TDD adapter/ingestion integration.
**Spec mapping:** AC2, AC9.
**Tests:** Four versions/same heading; same path across repos/scopes; updating,
failed/CAS-losing attempts; stale cached lexical/vector hits; private/expiry
changes; successful retry; preserved history/canonical digests. Unknown heads
are not selected by latest timestamp; unchanged scans verify missing heads.
**Approach:** Add neutral knowledge head ports, adapter-cell migrations and
source-read/ingestion activation around existing document writes. Update normal
candidate selection and embedding eligibility to consult current heads. Keep
explicit canonical/history access separate from candidate views. Cover imports
and scanner paths that create documents.
**Done when:** The obsolete backlog-body-loss chunk cannot be current evidence
on the retained store; sibling/independent owners and historical rows survive.

**Construction:** New SQLite head/lease tests cover retained versions, incomplete
inventory, CAS/attempt reuse, exact scope/source identity, immutable canonical
chunk content, code/generic lease-family exclusion and raw-mutation invalidation.
Scanner tests cover same path across repositories/scopes, private/expiry changes,
missing-head adoption and deleted generic files without a host manifest. Cached
lexical/graph fixtures validate stale projections and durable derived ownership.
Lossy generic legacy imports remain unverified; verified code-bundle imports
activate through the existing code-source writer and mirrored head. Existing
unmanaged generic recall remains compatible; T6–T9 must require a verified head
for every new code-evidence candidate. No new history/export format or PG
code-source support is claimed.

### T4: Token-aware structural segments and parent resolution

**Execution:** Implementation and focused construction gates complete; independent
adversarial/security and quality fallback re-reviews clean. Canonical methods
remain unchanged, exact model-tokenized segments cover their tails, and derived
vector namespaces preserve the configured SQLite file layout. Retry leases fence
stale workers; missing/failed inventories recover, and unavailable source siblings
are skipped. The release stdio/BGE smoke and full coverage/actual-vector tests are
recorded in `reviews/t4-2026-10-14.md`. Context/category enrichment remains T5;
pinned offline model asset identity remains T7, and final quality/self-store gates
remain T10–T11. No beta-readiness or ranking improvement is claimed here.

**Depends on:** T1
**Mode:** TDD pure and model-tokenizer integration.
**Spec mapping:** AC3–AC4, AC9.
**Tests:** Complete Unicode byte ranges; nested/large callables; context/residual
units; oversized literals/continuations; special-token budgets; parser variants;
representation-keyed identity; source transition and excluded snapshot donors.
**Approach:** Extend normalized range output from the current Tree-sitter provider
and add focused segmentation behavior in `core/code`. Model adapters supply exact
token boundary/count information. Persist knowledge-derived segments through
neutral ports; adapt vector canonical resolution to parent ownership. Reindex
uses guarded current representation inventories and bounded existing backfill.
**Done when:** Canonical methods are unchanged and every eligible range is covered
or explicitly unsupported; stale/incompatible segment IDs resolve to no candidate.

### T5: Authorized context and representation compatibility

**Execution:** Complete: construction, independent re-reviews, real-model live
smoke and all three frozen development query-format comparisons. The predeclared
mean-MRR@10 rule selects `none`; its small top-ten recall tradeoff is recorded in
the ablation artifact. Defaults/annotations now agree; generic document query
formatting remains unchanged. See `reviews/t5-2026-10-15.md`. Final pipeline quality
and pinned offline asset identity remain their T7/T10 gates.

**Construction design:** Add pure AST-derived per-unit category/signature/import
ranges in `core/code`; paths alone must not label inline tests as implementation.
Select bounded current facts in integration from independently authorized canonical
contributors. Preserve raw source references; never infer resolved dependencies
from syntax-only imports. Revalidate every contributor at persistence, inference
and query resolution. Facts that do not fit are omitted whole; actual combined
input is retokenized, and context content participates in segment identity.
Version explicit query formatting separately from passage identity; preserve the
legacy generic document query behavior. Run all three query-prefix variants on
development data only before choosing the release format. Actual asset digests
and offline provisioning remain the already-planned T7 work.

**Depends on:** T2, T4
**Mode:** TDD and live embedding ablation.
**Spec mapping:** AC3–AC4.
**Tests:** Parent/import facts with mixed visibility, unresolved edges, context
budget exhaustion, source changes during inference, changed tokenizer/context
hashes, unchanged retry and all three BGE query-prefix variants.
**Approach:** Build bounded deterministic context from current normalized units
and authorized graph facts. Add passage/query representation identities and
model-token checks; keep code processing local. Preserve existing source
authorization helpers, not backend flags as permission.
**Done when:** Real tail vectors differ and query representation is selected from
development evidence with reproducible model/tokenizer identity.

### T6: Candidate depth, fusion and bounded graph expansion

**Execution:** Complete. Pure ranking/pool tests and nine real-authority candidate
fixtures pass. The real offline BGE candidate probe admits 50 hits per initial
lane despite final limit ten, deduplicates sources/segments and reconstructs RRF
traces; 85 parents remain before scoring. A measured per-hit whole-store ownership
scan was replaced by equivalent target-bound checks. Full indexed calls take
1.27/0.95 seconds at lane caps 50/20; final evidence/p95/quality remain later gates.
See `docs/perf/reliable-code-candidate-2026-10-15.md` and the raw live artifact.

**Construction:** Named recipe lane clones feed a neutral, request-bound code
candidate stage. Vector hits retain segment identities; entity/chunk hits resolve
through current source/head inventories and canonical segments. Existing weighted
RRF fuses the bounded lanes; expansion keeps a separate trace using the configured
graph weight. Its neighbor and adjacency-call budgets are global, including empty
lists. Pure scorer-pool/parent aggregation helpers preserve first-parent order,
round-robin spare slots, maximum segment score and full RRF fallback. Scoring,
evidence assembly and transport wiring remain their later tasks.

**Depends on:** T2, T3, T4
**Mode:** TDD plus real-store candidate traces.
**Spec mapping:** AC2, AC5.
**Tests:** Final limit 10 still retrieves configured per-lane depth; disabled
lanes perform no calls; zero weights are not falsely considered disabled;
anchor-only graph bounds, segment/parent dedup, stable ties and unchanged generic
memory behavior. No unauthorized candidate reaches the scorer pool.
More parents than scoring slots, multiple segments per parent, scorer cutoff,
no-scorer and failed-partial-scoring orderings have distinct tests.
**Approach:** Use existing indexes, weighted RRF/composer and canonical resolvers
in focused shared provider operation stages. Cap before model scoring, aggregate
parents afterwards. Instrument phase costs before performance changes.
**Done when:** Candidate recall and final output have independent typed budgets
and packet provenance remains attached to the actual contributing units.

### T7: Real local ONNX scorer and asset lifecycle

**Execution:** Component complete. Pinned BGE/Jina assets were explicitly
provisioned; actual offline encoder and scorer tests pass without observed
internet socket/connect attempts. Jina changes ranking, preserves batch alignment
and scores an untruncated 622-token pair. Both independent implementation reviews
are clean after preserving the generic recall fallback contract. The real BGE
namespace reindex preserves canonical/history digests. Code evidence invocation,
transport readiness, calibration and final quality remain T8–T11 work.

**Construction design:** Share exact asset-byte verification between embedding
and scorer adapters in a focused `adapters/models/local-assets` crate. It owns
admitted manifests and cache-confined offline reads only; an explicit provisioning
script handles verified cache import or allowlisted downloads/redirects. Inference
receives verified bytes, including any declared external-data files. Move the
existing scorer port to `core/retrieval` with an old-path re-export, adding
fail-closed locality/identity/token inspection and compatible batch defaults.
Installed FastEmbed 5.17.2 exposes the scorer tokenizer: disable truncation,
correct Jina padding from its actual tokenizer pad ID and count complete pairs
before inference. The library returns sorted raw logits with original indices;
the adapter remaps and validates them. Independent structural/security placement
review is clean. Official pinned metadata and a Rust compile/runtime probe remain
the library contract oracle; configured names alone never confer readiness.

**Depends on:** T1, T2
**Mode:** TDD stubs plus actual-model integration.
**Spec mapping:** AC6.
**Tests:** Asset hash/revision checks, offline missing cache, pair-token/model
bounds, batch result remapping, NaN/count/index errors, local-only execution,
corrupt/load/inference failures and stable trace preservation. A realistic
multi-candidate code fixture changes ranking with actual inference.
An externally declared/injected scorer receives zero code inputs; a model-name
alias does not confer locality or source authorization.
**Approach:** Add feature-gated focused model/asset/tokenization modules in
`adapters/retrieval/cross-encoder-rerank`; reuse FastEmbed/ONNX and injected scorer
interfaces. Pin the official model before fetching for live tests. Wire real
scorer construction through supported recipe composition into the code scorer
handle. Generic recall retains its previous RRF fallback: its potentially long,
unbounded inputs do not inherit this code-specific scorer admission contract.
**Done when:** Recorded local scores come from real ONNX pairs, not word-overlap
stubs, and normal invocation performs no network access.

### T8: Shared evidence operation and diagnostics

**Execution:** Component complete. Fifteen evidence integration tests, two assembly
budget tests and independent adversarial, quality and security re-reviews passed.
The real offline BGE/Jina probe after review fixes returned 10 current items from
42/30 scored pairs in 12.36/7.61 seconds; exact excerpt bytes, known indexed
revision, complete token coverage and accepted-schema validation passed. Missing
calibration correctly returned insufficient evidence. This is a component proof,
not the held-out quality or final p95 gate. See the dated T8 review reports and
`docs/research/reliable-code-intelligence-evidence-live-2026-10-15.json`.

**Construction design:** `code_evidence` contains focused preflight, scorer-view
preparation, ranking, calibration, assembly and operation modules. Byte admission
and actual embedding/scorer query budgets precede every inference; locality is
checked before any injected scorer inspector receives a query or code. Only
implementation parents enter the scorer pool; other categories remain bounded
supporting units. Oversized pairs split exact source ranges using untruncated
scorer offsets and complete-pair checks, with whole context facts omitted only
when needed to fit. Continuation views share their original embedding segment;
compatible optional `rerankSources`/`rerankContext` preserve exact successful pair
inputs separately from original embedding coverage. Pooling runs across views
with existing one-parent-first/round-robin helpers, at most the configured pair
count; aggregation emits only the best scored view per embedding segment.
Calibration is injected as a typed immutable artifact; its runtime binding
hashes effective settings, actual model/tokenizer identities and both input
representation contracts. Continuation preparation is lazy and bounded by the
pair-work cap: the existing pool selects original segments first; prepare the
first exact view of each selected segment in pool order, then spend remaining
slots on continuations round-robin by prior parent order. A cursor records source
remaining without enumerating every future view. Diagnostics distinguish omitted
original segments/parents, prepared-but-unscored views and original segments with
unprepared continuation tails; unknown future view counts are never invented.
Request overrides invalidate a nonmatching artifact.
Threshold follows parent aggregation and precedes diversity. Assembly uses exact
range excerpts, counts every emitted source/context token, enforces all array
and serialized-byte bounds, and reports omissions. The operation compares the
source/index revision after all inference and assembly, retries at most once,
then returns only re-resolved current ranges or drops the affected packet;
request caches never grant authorization. The recheck covers exact heads,
policies, attempts, generations, representations and every supporting/context
donor, not Git revision alone. Changed bytes never inherit an old score: retry
the complete pipeline once or drop the evidence. Internal view IDs are unique
and retain their original segment mapping; stable maximum-view ties preserve
prior RRF/view order. Original-segment omissions and extra unscored views have
separate counters. Scorer-view rendering, separators and continuation rules have
a versioned identity included in calibration. A winning score, coverage, ranges
and facts are one coherent record; if budgets cannot retain it intact, omit it
rather than trim input references while retaining the score. Evidence status is
computed from final post-budget/post-recheck primary items only. Scope/requester
stay trusted host input. Numeric preparation/attempt counters survive model
fallback while all views/scores/source text are discarded. Source-change retries
retain only discarded-attempt pair counts and retry count; final per-view counts
refer to the retained attempt. Current indexed source/document revision metadata
is obtained through an exact-scope, retrieval-authorized knowledge port and
rechecked with source authority; it is never inferred from the live Git checkout.

**Depends on:** T5, T6, T7
**Mode:** TDD and source-grounded integration.
**Spec mapping:** AC5–AC7.
**Tests:** Scored segment-to-parent aggregation, tail/guard excerpts, supporting
unit selection, exact references, total token budget, final source recheck,
retry/drop on source change, requested/effective scorer state and both failure
policies. Metadata from denied sources is not exposed in failure details.
Missing/stale calibration, overrides, calibrated fast and fallback test status/
threshold rules; serialized token/byte/array limits and excerpt correspondence
are checked together.
**Approach:** Add a thin `EngramProvider` operation backed by focused eligibility,
retrieval, ranking, evidence and diagnostic modules. Source evidence is current
and bounded. Use existing failure/provenance contracts; retain full source-read.
**Done when:** Valid packets contain inspectable current evidence and truthful
insufficiency/fallback/representation status rather than only symbol headers.

### T9: Rust/native/HTTP/stdio parity and capability readiness

**Execution:** Component complete. Same-corpus actual Rust/native/stdio/HTTP
calls each scored 42 pairs and returned the same typed packet (phase timings
excluded; the Rust diagnostic JSON wrapper's score numbers compare by declared
f32 bits). Native/stdio/HTTP item JSON values match exactly. After readiness
review fixes, actual native BGE/Jina tests passed 23/23 and real stdio scored
three pairs on a fresh fixture. Strict wire/immutable authority, missing/disabled
lanes and generic scorer readiness regressions pass; Rust library 76 + evidence
19, stdio suite 174, HTTP 83 + one explicit model skip, full TypeScript suite,
workspace check/build and boundary gates passed. The initial full TS run saw
five transient live-agentzero 503 failures; isolated route returned 200, graph
7/7 passed, and the unchanged full test command passed on retry. No store was
wiped or test weakened. Calibration/quality and retained-store gates remain T10/T11.

**Construction design:** All surfaces call the same Rust `code_evidence` method.
Native handles bind a validated trusted `CodeSourceContext` at construction;
query JSON contains only the accepted strict query DTO. HTTP exposes `code`
with `op=evidence` and the same query fields. Stdio removes only `op` before
strict DTO decoding and uses launch scope plus its retrieval requester. Two
additive capability keys distinguish `code_evidence` prerequisite handles from
`code_scorer` actual admitted local model readiness; neither certifies calibration
or corpus quality. Configured missing lanes report degraded prerequisites, while
source/embedding authority missing reports unsupported. Calibration is loaded
once from an explicit operator-selected JSON path (64 KiB, regular file), never
from query inputs or repository discovery, and all recipes install the immutable
artifact. Existing packets still validate its identity at request time. The native
build propagates the local scorer feature; generic recall remains unchanged. A
compatible port declaration `supports_code_input` defaults false for generic
scorers and is true only after actual local input-contract construction. Readiness
and operation admission require it alongside actual identity; disabled vector
lanes impose no vector prerequisite, while missing enabled lanes degrade.

**Depends on:** T8
**Mode:** goal-based end-to-end integration.
**Spec mapping:** AC1, AC6–AC8.
**Tests:** Same corpus/config across all four surfaces; actual scorer use,
capability readiness from handles/assets, strict wire override denial, malformed
query limits and equivalent config diagnostics. Compile features propagate through
N-API and recipes. Unsupported backends degrade explicitly, not silent empty.
**Approach:** Add generated native DTO calls and typed TS wrappers, stdio
`code op=evidence` and the HTTP operation; transport adapters remain thin.
**Done when:** Both embedded Rust and native TS callers exercise the actual
operation and all supported recipes honestly describe available prerequisites.

### T10: Calibration and profile validation

**Execution:** Evaluation harness built and independently reviewed clean;
26 synthetic construction tests and seven Rust evaluator tests pass. The
actual label-free observer smoke preserves exact source bytes with local
scoring. Derived evaluation namespace preparation is running; development
calibration and the untouched held-out assessment have not run. This checkpoint
does not establish quality or operational acceptance. See
`reviews/t10-harness-adversarial-2026-10-15.md` and
`reviews/t10-harness-quality-2026-10-15.md`.

**Predeclared evaluation design (before held-out assessment):** Retain the frozen
T0 baseline artifacts and binary. Prepare only derived namespaces in the three
existing evaluation stores; verify corpus bytes and before/after canonical table
digests. Compare `segmented` (balanced settings with rerank none) and `balanced`
(the unchanged balanced profile) over identical eligible candidate orders, plus
the separate default `fast` profile. No new parameter search or model selection
is planned: T5 already selected query formatting on development data.
A label-free Rust development observer collects actual eligible implementation
parents in pre-score stable RRF order, actual operation packets and unsealed
binding templates from the same immutable store/config. Observer overhead is
reported separately and cannot substitute for MCP latency measurements.
Resolve each judged unit by exact pinned path/qualified anchor/declaration ranges
against canonical indexed metadata; ambiguous or missing identities fail the
harness. Parent metrics use the existing `engram-eval` function through a line
protocol example. Baseline headers are reconstructed conservatively from frozen
canonical metadata and the original renderer contract; ambiguous headers remain
unjudged, and only actual emitted source excerpt bytes count for range coverage.
Every ambiguous/unmapped legacy header retains its original rank as an unjudged
placeholder; never compress positions before top-ten MRR/nDCG/recall.
Legacy rank metrics are historical diagnostics, never validated new-profile
evidence. Baseline candidate/phase metrics remain unavailable.
Development calibration uses only the 45 fixed development cases. For each
strategy/profile, select the lowest finite f32 threshold that makes at least
ceil(0.90 * development negative count) negative packets empty of primary items.
The negative maximum is the largest parent strategy score among actual
post-budget inspection primary items; no-primary negatives are rejected at every
threshold. Any effective lane/model/authority failure (apart from the explicitly historical
`code_sources` unavailability diagnostic), reindex requirement, wrong strategy/state or CE
fallback aborts fitting instead of counting as a rejected negative. Candidates
are the least finite f32 (-MAX, which admits every finite score) and nextUp-f32
values above distinct finite negative maxima; equal scores are rejected together
only above the tie. Signed zero/subnormal values follow f32 nextUp semantics.
Overflowing nextUp values are not admissible thresholds; if no finite threshold
meets the fixed negative denominator, fitting fails without dropping cases. No arbitrary grid or held-out
feedback is used; the least restrictive qualifying threshold preserves the most
ranked positive candidates. Seal the actual binding template using the Rust
artifact sealer and the digest of development labels in fixed query order.
Offline fit diagnostics are explicitly inspection metrics; rerun the real
operation with immutable installed calibration to measure validated development
metrics. If any input/identity/baseline fails validation, do not advance.
Require the exact calibrated development case inventory and at least 14 of
the fixed 15 negatives to return both insufficient status and empty primary
items for every variant. Record the actual counts and independently recompute
them before freezing; validated artifact IDs alone do not satisfy this check.
Then persist a freeze file binding executable, configuration, calibration,
manifest, model identities and runner hashes before semantic selection/use of held-out queries.
Combined label-file bytes are hash-verified as an opaque envelope in all phases;
only the selected split's judgments are traversed or source-range verified.
Development never traverses held-out queries/judgments. The freeze additionally
binds the resolved assessment/base roots, exact runtime artifact/store path sets,
and one fixed assessment marker, plus logical persisted rows (including vector shadow
blobs) and the actual Tantivy lexical file/directory inventory (hash/length for
every regular file, metadata and namespace marker). Only the installed Tantivy
0.22.1 transient `.tantivy-writer.lock`/`.tantivy-meta.lock` names are excluded;
unknown files and additions/removals change the freeze. Symlinks/nonregular
lexical entries are rejected. These inputs are bound for baseline and new stores; verify these before label release and after
assessment, independently of SQLite WAL/checkpoint byte layout.
Every variant must retain the exact case inventory and succeed, including
baseline startup/warm-up; a failed baseline or fast run invalidates the overall
assessment even if balanced positive scores pass. An uncorrelated observer
transport becomes poisoned on timeout, EOF, write failure or malformed output;
close/reap it before restarting for another case so late replies cannot shift
query labels. Failed cases remain in their fixed denominator.
Checkpoint failed cases and any rejected complete responses before metric
processing, teardown or restart; retain stderr from every child, including
failed replacement construction.
One untouched held-out assessment runs baseline/segmented/balanced/fast with no
retuning. Record actual packets, candidate order, per-query ranks/excerpts,
macro/per-repo metrics, negatives and paired MRR/nDCG/range coverage. A failed
quality gate leaves the feature NotShipped; it does not authorize a revised
threshold against the same held-out labels. The fixed held-out query order also
predeclares the >=60 measured balanced MCP calls for T11 after one warm-up per
corpus; actual host calls, not observer calls, determine warm p95. Use nearest-rank
p95 sorted[ceil(0.95*N)-1] over all 60 measured calls; a failed/timed-out call or
incomplete run fails the operational gate and cannot be silently excluded.

**Depends on:** T0, T9
**Mode:** goal-based scored retrieval evaluation.
**Spec mapping:** AC10–AC11.
**Tests:** First require completed, manifest-bound original/identifier and
three-corpus development baselines from T0; no tuning starts on partial results.
Development calibration selects thresholds without reading held-out
labels. Original and held-out queries run paired baseline/segmented/scored
configurations. Scores are not probabilities; negative packets do not assert
absolute absence. Candidate/final recall denominators and required-anchor
coverage follow the frozen manifest.
The manifest instantiates the spec's fixed parent/range counting stages, macro/
per-repo means, graded gains, unjudged handling and separate negatives; freezing
cannot redefine the formulas. Unvalidated/fallback packets fail positive gates.
**Approach:** Run controlled parameter/representation/scorer ablations on dev
only. Pin calibration/model/config hashes, then perform one frozen held-out
assessment. Publish raw per-query ranks/citations, aggregate metrics and failures.
**Done when:** All positive/negative/nonregression targets pass. Unmet quality
leaves the profile/feature unshipped; any target change requires user sign-off.

### T11: Retained-store rollout, operational proof and gates

**Execution:** Consistent online backup verified before any self-store rollout;
original canonical/history data remain retained. Whole-workspace Rust tests
pass (1,181 passed, 10 ignored), and the rebuilt native addon passes five live
tests including actual local embedding/scoring. TypeScript, build, generated
contracts and boundary gates pass. The final observability correction is
independently reviewed clean; integrated construction review is clean with
T10/T11 outcome gates explicitly pending. Retained-store scan/reindex/evidence
verification has not run. See
`reviews/t11-observability-adversarial-2026-10-15.md`,
`reviews/t11-observability-quality-2026-10-15.md`, and
`reviews/whole-spec-construction-quality-2026-10-15.md`.

**Construction correction:** Full workspace testing found that observability
captured capability flags before the provider computed actual code readiness.
Keep the existing conformance assertion unchanged: diagnostics must equal the
provider's complete report. At the provider boundary, delegate diagnostic counts
and timings while supplying the finalized immutable capability snapshot. Verify
the existing conformance fixture and a focused supplied-handle regression; this
applies equally to every backend and does not change retrieval behavior.

**Depends on:** T10
**Mode:** goal-based live verification and repository gates.
**Spec mapping:** AC9, AC12.
**Tests:** Consistent backup/table digests, derivative activation/retry/idle
reindex, byte-exact self reads and zero unauthorized/snapshot/obsolete current
hits. Warm >=60 predeclared requests on frozen CPU setup yield p95 <=30 seconds;
cold load/phase/index growth are separately reported. Profile before optimizing.
**Approach:** Stage adapter migrations, verify source/representation inventories,
provision pinned model explicitly and switch only code-evidence reads to the new
profile. Retain canonical data and temporal baseline. Run mandatory gates below
and `engram-live-verify`; repeat independent reviews after fixes.
**Done when:** All ACs have evidence and the spec can truthfully change to Shipped.

## Rollout and verification

Before production implementation: `scripts/ci/check-contracts.sh` and
`scripts/ci/check-engine-neutrality.sh`. Implement each task as a reviewable
slice; an intermediate task is not the completed feature. Adapter migrations
precede consumers; model provisioning precedes actual-model tests. Missing
prerequisites are reported, never hidden by a completed status.

Required handoff: `cargo fmt --all`, `cargo check --workspace`, focused and
workspace tests, `pnpm run contracts:generate`, `pnpm run typecheck`,
`pnpm run test`, `pnpm run build`, contracts/docs/neutrality/surface-parity gates
and real model/source/store verification. Reports distinguish pre-existing
advisory/lint baselines from introduced regressions.

Rollback selects fast/no-scorer or existing generic retrieval, with explicit
unvalidated/degraded state; it does not delete source/history or represent
rollback as passing the reliable-profile quality gate. New schemas/default
profiles/calibration identity are versioned and retained for reproduction.

## Risks

Actual code relevance of the chosen English scorer is unverified; failed gains
block shipment or require reviewed model/profile changes. More segment targets
increase index/model work; per-request canonical authorization can dominate
latency, so measure phase costs before optimizing. Document-head migration may
make legacy docs temporarily unavailable until verified rescan. Calibration does
not generalize to arbitrary operator overrides; those report unvalidated status.
The architecture reference contains historical runtime statements; actual
checkout/accepted ADRs govern construction, not stale descriptive paragraphs.

## Changelog

- 2026-10-14: Draft authored from measured self-evaluation and primary-source
  research; real local scorer and configurable defaults explicitly in scope.
