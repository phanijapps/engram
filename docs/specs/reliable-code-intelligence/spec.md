# Spec: Reliable natural-language code intelligence

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [plan.md](plan.md)
- **Constrained by:** ADR-0003, ADR-0009, ADR-0022, ADR-0026, ADR-0028 (pure-code ownership), ADR-0029, ADR-0030; RFC-0021
- **Brief:** none — authored directly from the user's approved research direction
- **Contract:** [accepted additive v1 schema](../../../contracts/v1/schemas/engram-v1.schema.json); [domain extension](../../domain-data-model.md#reliable-code-intelligence-contracts); existing [RecallFusionConfig](../../../contracts/v1/schemas/recall-fusion.schema.json)
- **Shape:** mixed (data, retrieval behavior and model/transport integration)

> Spec contract: these are requirements, not claims about current runtime
> behavior. Approved by the owner on 2026-10-14; implementation and public
> contract promotion are in progress.

**Assessment outcome (2026-10-15): Not shipped.** The frozen assessment is
complete: held-out required-unit Recall@10 is 60% (target >=80%), with Engram
60% and Vite 46.67% below the >=70% per-repository target. Original questions
recover all required anchors in 1/10 packets (target >=8/10). All 356 calls
completed; warm actual MCP p95 passed at 10.800065 seconds. AC10 is unmet;
actual-self rollout/reindex and its AC9/AC12 proof were not executed because
quality prerequisites failed. Construction evidence does not imply acceptance.
Recorded human label/citation review remains pending. See the
[final assessment](../../research/reliable-code-intelligence-final-assessment-2026-10-15.md).
The Implementing status and unchecked acceptance criteria intentionally remain;
this failed acceptance run does not authorize target changes or held-out tuning.

## Objective

Developers, agents and product analysts ask natural-language questions and
receive current, authorized implementation evidence with inspectable source
ranges and explicit limitations. Final top-ten evidence recovers at least 80%
of labeled required units on the frozen held-out suite and all required anchors
for at least eight of the ten original difficult Engram questions. Exact-symbol
controls remain rank one. Operators tune one typed configuration through
profiles, a file and environment overrides. A real local cross-encoder scores
query/code pairs; actual execution and fallback are visible. Complete source
preservation and authorization remain intact.

## Boundaries

### Always do

- Reuse `RetrievalIndex`, RRF/`compose_context`, `RecallFusionConfig`, source
  authorization helpers and the existing `RerankScorer`/reranker ports.
- Keep complete code canonical; derive bounded embedding segments with typed
  parent/range/generation/representation identities. Preserve stored history.
- Run inference locally; authorize every context contributor before model use
  and every returned source immediately before packet publication.
- Expose equivalent Rust SDK, native TypeScript, stdio MCP and HTTP behavior,
  generated contracts and truthful capability/failure diagnostics.
- Record effective settings, model/representation identity, input coverage and
  timing. Evidence status is not a probability derived from RRF.

### Ask first

- Change the audience, add generated Silver summaries/product requirements,
  remote inference or a different default scorer family.
- Weaken frozen targets, revise held-out labels after seeing results, change
  generic memory defaults or introduce breaking v1 fields.
- Add unrestricted hot reload, arbitrary model downloads or another storage
  engine. Accepted schema promotion follows normal contract review.

### Never do

- Delete canonical data/shared databases to rebuild vectors, claim cross-store
  ACID or activate partial source/representation inventories.
- Put model, SQL, filesystem or environment-reading behavior in `engram-domain`,
  pure `engram-code` or deterministic retrieval algorithms.
- Duplicate fusion, create a god module/TS domain implementation or authorize
  candidates using cached secondary-index flags alone.
- Treat snapshots, obsolete docs, tests, proposals or unverified legacy code
  as production implementation proof; tests can support an explanation.
- Claim a scorer ran when it fell back, upload source remotely, execute remote
  model code or silently represent a whole long method by one prefix.

## Behavior contract

### Shared configuration

`CodeIntelligenceConfig` is an optional code-specific member of `EngramConfig`.
The new code-evidence operation uses `balanced-v1` when configuration is absent;
generic memory/recall operations retain their defaults. Partial settings share
one Rust definition. Profiles are immutable versioned bundles; changed presets
require a new version and comparison evidence.

Field precedence is profile defaults < explicitly selected file < namespaced
environment < explicit launch/SDK fields. Profile selection itself follows
file < environment < explicit input; its defaults expand before field overrides.
Omitted CLI options do not overwrite file values. Unknown profiles/fields/env
suffixes, invalid types/nonfinite weights and dependent/model-budget violations
return typed errors identifying the key and source.

Hosts resolve once. `--config <path>` / `ENGRAM_CODE_CONFIG` select an explicitly
trusted JSON/TOML file, not an implicitly scanned-repository file.
`ENGRAM_CODE_PROFILE` selects a profile; nested overrides use
`ENGRAM_CODE__SECTION__FIELD`, such as
`ENGRAM_CODE__RETRIEVAL__CANDIDATES_PER_LANE=100`. Config fields are snake_case;
operation DTOs are camelCase. Diagnostics expose winning origins and the hash
of canonical resolved settings plus model/calibration identity.

The code profile's `fusion` reuses `RecallFusionConfig`, including its sole
reranker-strategy selector. `local_scorer` supplies model/budget/failure settings,
not another strategy field. Existing `<storage_path>/.engram/recall.json` keeps
its generic-recall discovery contract; it is not silently merged into code
settings or described as automatic repository-root discovery.

| Setting | `balanced-v1` | `fast-v1` |
| --- | --- | --- |
| Body token target / maximum context | 384 / 64 | Same representation |
| Query instruction | none (development-selected) | none (development-selected) |
| Initial lanes | vector, lexical | vector, lexical |
| Candidates per lane / final parents | 50 / 10 | 20 / 10 |
| RRF k / weights | 60 / 1.0 | 60 / 1.0 |
| Reranking / maximum scored segments | cross_encoder / 50 | none / zero model calls |
| Local scorer | `jinaai/jina-reranker-v1-turbo-en` | Not loaded for reranking |
| Scorer query / total pair tokens | 128 / 1,024 | Not used |
| Scorer batch / failure policy | 8 / degrade | Not used |
| Anchored graph expansion | One hop; 20 neighbors total | Disabled |
| Evidence tokens / supporting units / per-file parent cap | 6,000 / 20 / 4 | Same |
| Startup/query model download | Disabled | Disabled |

These are draft starting presets. `balanced-v1` ships only after AC10–AC12
pass; `fast-v1` is an opt-in/control profile with separately reported quality
and calibration, not a substitute for that gate. A failed gate does not permit
silently changing values or retuning
held-out data. Runtime validation includes actual tokenizers/special tokens,
model window compatibility and positive contribution from enabled lanes.
Per-request overrides are allowlisted and cannot exceed resolved operator bounds.

The code operation accepts only `none` and `cross_encoder` rerank strategies.
It rejects `mmr`, `llm_judge` and `policy_priority` before initialization or
inference; generic recall retains its existing strategy contract. Graph lanes
in `enabled_lanes` are independent candidate generators. `graph_hops`/neighbor
limit govern separate expansion from retrieved anchors: balanced disables graph
candidate lanes but expands one hop; fast expands zero. Zero hops skips expansion;
positive hops require a positive global neighbor limit.

Embedding inputs satisfy `codeTokens + contextTokens + specialTokens <=
embeddingWindow`, with code/context within configured targets. Query tokens plus
instruction/special tokens fit the embedding window. Scorer pairs satisfy
`queryTokens + codeTokens + contextTokens + specialTokens <= pair_token_limit <=
scorerWindow`; scorer query count is at most `query_token_limit`. Over-budget
queries return InvalidRequest before inference. Segments too large for scoring
use deterministic range-based continuations, never prefix truncation.
`result_limit <= candidates_per_lane` and, for cross-encoding,
`result_limit <= max_candidates`. Fewer candidates can yield fewer parents;
batch size never increases the scored-pool bound.

### Current source and documents

Current means the current **indexed** generation, not a claim that disk was
observed at this instant. Evidence exposes indexed revision when known, source
hash and generation; unknown revision is explicit. Changes during inference or
assembly cause reauthorization and removal/retry of affected evidence.

`DocumentHead` identifies one current document per exact scope/repository/stable
source. Updating/incomplete/unverified heads produce no current candidates.
Canonical inventory verification and attempt/generation CAS precede activation.
Secondary vector/lexical/cache hits resolve against the head. A failed refresh
remains unavailable until a verified retry; it cannot reactivate stale content.
Historical records survive. New history-query operations are outside this slice;
existing explicitly authorized reads retain their contracts. Legacy heads need
verified scan/import evidence, not timestamp-only selection.
Existing unmanaged generic recall stays compatible; it does not confer verified
current-head authority on the new code-evidence operation.

### Token coverage and context

Every nonempty canonical code unit has complete derived source-range cover or
an explicit unsupported/failed representation state. Coverage includes method
tails and context/residual units. Every model input fits its actual tokenizer
window including context/special tokens; no silent model truncation occurs.

Segments prefer AST statement/block boundaries. Oversized terminal nodes use
marked continuation ranges and parent identity. Descriptors record parent,
source generation/attempt, order/ranges, representation ID, token count and
continuation status. Complete methods/snapshots remain canonical; snapshots do
not enter semantic/model feeds. Rust/TypeScript/Python fixtures exercise the
same normalized segmentation contract. Parser registration/compiler adapters
are separate slices.

Deterministic bounded context includes path, qualified symbol, signature/parent
and resolvable relevant import/containment facts. Every contributor is current
and authorized; unresolved dependencies are explicit. No generated summary or
inferred requirement substitutes for source.

Passage segmentation/context/tokenizer/model/normalization changes create a
new representation identity and target IDs. Query formatting has its own
version; query-only instruction changes do not re-embed unchanged passages.
`query.instruction` admits `legacy-v1` (initial default), `none` (selected default) and
`bge-retrieval-v1`; the latter uses the documented BGE retrieval instruction.
The frozen three-corpus development ablation selected `none` using its predeclared
mean-MRR@10 rule; [the recorded comparison](../../research/reliable-code-intelligence-query-ablation-2026-10-15.json) includes the recall tradeoff. This is a compatible default
selection for the unshipped profiles; held-out scoring remains a later gate.
Reindex-required state is observable; incompatible/snapshot/legacy hash donors
cannot satisfy new-profile readiness or reuse.

Segments are separate knowledge-owned derived records, not new canonical
source roles. Their resolution validates the current canonical parent's source
inventory and each context contributor. Parent policy does not become a
blanket permission for independently owned context. Existing source-role enum
meanings and canonical contribution inventories remain unchanged.

### Candidate pipeline and real scorer

Per-lane depth is independent of returned parents. Enabled lexical/vector lanes
prefetch, resolve authorized canonical targets and fuse through existing weighted
RRF. Graph expansion is anchored and bounded across the whole request. Disabled
lanes are not invoked; zero weight alone is not advertised as saving lane cost.

A local scorer ranks a bounded fused **segment** pool before presentation.
Group stable RRF-ordered segments by parent; select the best segment of each
parent in first-appearance order, up to `max_candidates` parents. Fill remaining
slots with subsequent segments in round-robin parent order. Remaining segments
and parents are excluded from scored ranking and counted as budget omissions.
No more than `max_candidates` pairs are scored;
pair/batch limits use the scorer's tokenizer. Presentation selects matching
ranges plus bounded supporting units instead of a fixed method-prefix excerpt.

Successful cross-encoding ranks parents by maximum scored segment score, with
ties retaining prior RRF parent order. Unscored parents are not mixed into that
ranking. Strategy `none` ranks all eligible fused parents by maximum segment
RRF, retaining first appearance on ties. Failure discards partial rerank scores
and uses that full bounded pre-score RRF parent ordering. Raw cross-encoder and
RRF scores are never compared. Relevance filtering precedes diversity/output caps.

The scorer is a real in-process ONNX model behind `RerankScorer` and
`RetrievalReranker`. An admitted manifest pins revision, artifact/tokenizer
SHA-256, license, pair-input contract and score transformation. Installation is
an explicit operator action using an allowlisted manifest; startup/query performs
no downloads, uploads or remote-code execution. Invalid shape/count/nonfinite
model results fail validation. Work is bounded; no hard cancellation of an
already-running synchronous model batch is implied.

The injected scorer declares local execution explicitly; unknown/external
implementations default to nonlocal and receive no code input from this
operation. A custom SDK scorer cannot inherit authorization merely from the
configured model name. Generic non-code reranking retains its port contract.

Missing/unavailable/failed scorer with `on_failure=degrade` preserves authorized
pre-rerank order and reports the failure; `error` returns typed unavailable or
inference errors. Generic memory remains usable. Fallback does not count as
successful cross-encoder execution. Successful calls report the actual manifest
and segment scores, including in capability/diagnostic tests.

### Evidence operation and transport parity

Before tokenization, reject queries over 16,384 UTF-8 bytes and repository
identifiers over 2,048 UTF-8 bytes. These fixed admission limits apply through
every transport and embedded callers; oversized input invokes neither tokenizer
nor scorer.

Source-reference generations and hashes preserve the exact opaque strings in
accepted source contracts. Segment `parentChunkId` locates the canonical record;
`parentUnitId` and reference `unitId` identify its descriptor unit. Validate both
identities together; never reinterpret one as the other.

`CodeEvidenceQuery` accepts query/repository and bounded result/candidate/token
limits. Host scope/requester and index/model identity are not wire overrides.
`CodeEvidencePacket` contains distinct parent items, current source references,
query-relevant excerpts, supporting ranges, partial/omitted markers, existing
`RetrievalSourceFailure`s and requested/effective configuration diagnostics.
Budgets count all emitted excerpt/context tokens. A reference does not claim an
omitted full body was supplied. Existing authorized source-read retrieves it.

Evidence status is `candidates_available` or `insufficient_evidence`. The former
is not an existence/completeness/runtime-behavior verdict. Empty, denied or
below-threshold evidence is insufficient. A calibration artifact fixes its
threshold from development labels only and binds effective config, query/passage
representation, tokenizer/model/score transformation and strategy. Its score
space is successful cross-encoder parent score or, for `none`, parent RRF.
Thresholds apply after aggregation, before diversity/output limits. Only
production implementation parents satisfy the primary gate; tests, generated
code, docs and unknown classifications are supporting context. Classification
is normalized per-unit metadata with source provenance, including tests nested
inside production files, rather than extension-only inference.

With matching validated calibration, below-threshold parents are omitted and
status is `candidates_available` only if a primary parent remains. Missing,
stale/mismatched calibration, invalidating operator overrides or scorer fallback
set `unvalidated` and always `insufficient_evidence`. Authorized ranked candidates
can remain for inspection, marked unvalidated. No threshold from another
strategy/profile is borrowed; calibrated fast mode uses its own RRF threshold.
Insufficiency never asserts absolute absence or interprets normalized scores
as confidence probabilities.

Packets carry bounded per-segment source/range references, contributing fusion
traces, fusion/rerank scores, embedding/scorer token coverage, unresolved context
and structured excerpt parts mapped to exact UTF-8 source bytes. Supporting
units are separate from primary parents. Markers/derived context are separate
fields rather than alterations to source excerpts. Serialized packet size is
at most 256 KiB, with explicit omissions; token budgets include inspection
candidates and all emitted code/context.

The Rust facade operation and corresponding typed native operation back both
MCP transports: stdio `code op=evidence` and HTTP return the same packet.
Existing `code search`/`recall` remain compatible. Capability reporting separates
code-evidence prerequisites from actual local scorer readiness.

### Frozen evaluation definitions

For each answerable query, required units are canonical parent IDs located by
repository/path/qualified anchor in a pinned corpus; labels include required
source ranges and 0/1/2/3 relevance grades. Grade 0 is irrelevant, 1 useful
support, 2 relevant implementation and 3 a required implementation unit.
Labels and queries are never indexed. Negative queries have no required units
and are evaluated separately, not given artificial perfect recall.

Candidate Recall@50 uses the first 50 distinct eligible implementation parents
in stable fused-RRF first-appearance order, before scorer pool selection,
thresholds, diversity and output. Per-query recall is recovered required parent
IDs divided by required parent IDs. Final Recall@10 uses only the first ten
primary items, excluding supporting units/inspection-only unvalidated results.
Aggregate recall is the arithmetic mean across answerable queries, and per-repo
means use that repo's queries. Original-question success requires every expected
implementation parent for that question, not merely one helpful file.

Required-range coverage is separate: proportion of labeled required byte ranges
covered by exact excerpt parts in primary/supporting evidence; references without
supplied text do not count. MRR uses the first final primary parent of grade 2/3.
nDCG@10 uses gains `2^grade - 1`, log2 rank discounts and ideal ordering of pinned
judged parents. Unjudged parents have grade 0 and are flagged. Inspection-only
unvalidated results are excluded. Report macro means and per-query records;
no denominator is chosen after tuning. Negative success requires insufficiency;
fallback/unvalidated candidates cannot satisfy the positive quality gate.

Unjudged counts and expected-answer labels belong only to evaluation results,
not runtime packet diagnostics or scoring inputs.

## Testing Strategy

- **TDD, pure/adapter tests:** precedence/defaults, dependent budgets, source CAS
  and cached denial, complete range cover/context, identity/reuse isolation,
  stable scoring/aggregation and fallback; deterministic stubs exercise invariants.
- **Goal-based integration:** generated contracts, native build/type tests and
  four transport/config surfaces prove the same fixture/effective settings,
  rather than mock tool-registration shapes.
- **Goal-based real-store/model:** actual ONNX scoring, no query-time network,
  long-function tail distinction, frozen retrieval quality, phase/warm/cold
  latency and retained-store migration/reindex. Model-free tests cannot satisfy
  real-scorer/quality criteria.
- **Recorded human evidence review:** blind-check corpus labels and citations;
  inspect missing-feature/partial evidence behavior. No UI gate is implied.

## Acceptance Criteria

- [ ] **AC1 — Shared config:** partial JSON/TOML/env/explicit input resolve
  identically across transports; all defaults, precedence, unknown/bad keys,
  falsy values, dependent/model budgets and bounded overrides are covered.
  Generic memory and legacy recall-file behavior are unchanged.
- [ ] **AC2 — Current evidence:** stale doc/version/index/cache hits cannot enter
  normal packets. Current-head publication, failed refresh/retry, exact scope,
  expiry/private changes and source races pass; historical records survive.
- [ ] **AC3 — Complete model coverage:** TS/Rust/Python nested-callable and
  oversized-node fixtures retain exact source, complete derivative ranges and
  bounded actual tokens. Real cancel/refund tail fixtures produce distinct
  relevant segment vectors and correct top-ranked parents for both queries.
- [ ] **AC4 — Context/identity:** only current authorized context is embedded;
  unresolved imports are explicit. Representation changes need new targets;
  snapshot/legacy/incompatible donors fail. Query-prefix variants are compared
  and the selected representation is recorded.
- [ ] **AC5 — Bounded pipeline:** lane depth/disabling, graph expansion, scorer
  pool, parent/file/output budgets are independent and bounded; stable ordering,
  finite scores and provenance/identity preservation pass.
- [ ] **AC6 — Real local scorer:** a pinned model scores a multi-candidate code
  fixture and changes ranking appropriately through all four surfaces. Traces
  identify actual execution. Missing/corrupt assets and invalid outputs exercise
  both failure policies. Probes show no source/query egress, implicit download
  or remote model-code execution.
- [ ] **AC7 — Evidence/diagnostics:** packets validate against promoted schemas;
  source ranges/hash/revision resolve byte-exactly. Useful tail/guard ranges are
  returned. Budgets/omissions, config origins/hash, scorer/calibration/reindex
  state and lane failures are accurate; insufficiency does not assert absence.
- [ ] **AC8 — Surface parity:** Rust, N-API/typed TS, stdio and HTTP return
  equivalent fixture evidence/diagnostics. Scope/requester/model/index overrides
  are rejected before inference. Unsupported prerequisites are explicit.
- [ ] **AC9 — Safe reindex:** online backup/canonical table digests prove preserved
  source/memory/history/temporal data. Partial representations stay unavailable,
  retries converge, unchanged repeats infer nothing and snapshots stay excluded.
- [ ] **AC10 — Frozen positive quality:** original questions discover every
  required implementation anchor in at least 8/10 top-ten packets; exact-symbol
  controls both remain rank one. Held-out candidate Recall@50 >=0.90; final
  required-unit Recall@10 >=0.80 overall and >=0.70 per repo, using the frozen
  manifest's definitions/labels.
- [ ] **AC11 — Negative/comparative quality:** at least 18/20 held-out unanswerable
  or irrelevant queries return insufficiency. No packet asserts absolute absence.
  Baseline, segmented/no-scorer and scored runs report MRR/nDCG/evidence coverage,
  paired results and zero unauthorized/obsolete current hits. Scorer-backed
  overall nDCG@10 does not regress against reranking off on the same candidates.
- [ ] **AC12 — Operational validation:** warm `balanced-v1` p95 <=30 seconds at
  concurrency one on the frozen local CPU reference setup, with predeclared
  order and >=60 measured requests. Cold load, phase timings, token/model-call
  counts and index growth are separate. Required workspace/contracts/docs/
  neutrality/parity gates and live self verification pass; unmet targets leave
  the feature unshipped.

## Assumptions and compatibility

- Technical: Rust behavior/adapters and TS transports define ownership
  (AGENTS.md; architecture reference; checked 2026-10-14).
- Technical: typed fusion/loading and an injected model-less scorer exist
  (retrieval config, MCP bootstrap, cross-encoder adapter; checked 2026-10-14).
- Technical: installed FastEmbed 5.17.2 supports `JINARerankerV1TurboEn` and
  local pair inference; its official [Apache-2.0 model card](https://huggingface.co/jinaai/jina-reranker-v1-turbo-en)
  was checked 2026-10-14. Exact artifact pins are a construction task, not a
  claim of already verified model-backed quality.
- Product: natural-language code evidence, defaults/env tuning and a real local
  scorer are confirmed by user instructions 2026-10-14; generated product
  requirements remain separate.
- Process: contract-first domain ownership, reviewed Draft and parity gates
  apply (AGENTS.md, CONVENTIONS.md, ADR-0022/0026/0029/0030).

Promotion is an additive v1 wire change. Direct Rust literals/custom TS transport
implementations need source updates; existing required fields and generic
operations retain their meanings. The draft JSON Schema is directly authored
without a dedicated schema-authoring skill; mechanical validation supplements,
but does not replace, semantic/contract review.
