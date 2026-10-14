# Reliable natural-language code intelligence: research

> Discipline: applied (practitioner-pattern survey)

Date: 2026-10-14. Reference implementation: Engram at `98fda3c`, plus the
[adversarial self-evaluation](code-intelligence-self-evaluation.md).
Purpose: inform the next brief; proposed configuration and defaults below are
research recommendations, not implemented settings or an approved contract.

## Recommendation

Build an evidence retrieval capability with **one typed configuration contract,
versioned default profiles, a configuration file and environment overrides**.
Extend Engram's existing fusion/configuration seams. Make tuning reproducible
and observable, then choose production defaults from a held-out evaluation set.
Configuration alone cannot recover truncated code or make an obsolete document
current. Fix representation and candidate eligibility before tuning ranking.
[synthesis; moderate]

This combines the code-specific retrieval difficulties documented by
[CoIR](https://arxiv.org/abs/2407.02883), the context/retriever limitations in
[CodeRAG-Bench](https://arxiv.org/abs/2406.14497), and the staged, labeled tuning
approach in [Qdrant's tuning documentation](https://qdrant.tech/documentation/search-tuning/how-to-tune-hybrid-search/).
Downgrade: indirectness; these sources do not validate an Engram configuration.
CoIR and CodeRAG-Bench are distinct research efforts; cAST and CodeRAG-Bench
share authors and are not counted as independent replications.

## Engram reference point

These observations come from the checkout, not historical RFC descriptions.

| Area | Existing behavior | Consequence for the next brief |
| --- | --- | --- |
| Source and code embeddings | Complete callable source; local inference guarded by source ownership/generation; model input capped at 8,192 characters | Preserve these guarantees; add derived token-budgeted segments instead of replacing complete methods |
| Fusion | `RecallFusionConfig`: `rrf_k`, default/per-source weights, optional reranker | Extend the existing contract rather than introducing another fusion implementation |
| Defaults | RRF `k=60`, weights `1.0`, no reranker; MMR lambda `0.5` when selected | Keep a compatibility baseline; improved defaults need comparative evidence |
| Configuration loading | Rust SDK accepts TOML profiles and explicit typed config; stdio MCP discovers `<storage_path>/.engram/recall.json` | Do not document this as automatic repository-root discovery; add an explicit, consistent config path |
| Search assembly | Requested result limit flows into recall; subsequent per-file cap is two; chunk excerpt is the first 500 characters | Separate candidate depth, evidence selection and presentation budgets; a guard in a long method may be absent from the displayed prefix |
| Reranking | MMR can use the local embedder; default bootstrap's cross-encoder selector has no model scorer and falls back | Selecting a strategy is not proof that it ran; report actual availability and fallback |
| Retrieval quality | Two of ten hard questions located expected implementation; both exact-symbol controls found rank-one functions | Preserve identifier lookup while addressing paraphrases and multi-file evidence |
| Document freshness | Four versions of an audit heading coexist; an obsolete version was returned after a current scan | Introduce current-document selection, while retaining intentional history access |

Source anchors: [backfill](../../core/integration/src/embedding_backfill.rs),
[fusion config](../../core/retrieval/src/config.rs),
[SDK configuration](../../core/integration/src/config.rs),
[MCP config loading](../../mcp/engram-mcp/src/bootstrap.rs),
[search assembly](../../mcp/engram-mcp/src/codegraph.rs), and
[reranker construction](../../core/integration/src/sqlite/bootstrap.rs).
The measured quality/freshness findings and their limitations are in the
[evaluation](code-intelligence-self-evaluation.md). These are direct local
observations; they are not a cross-repository performance estimate.

## Findings

### 1. Preserve complete code, embed bounded structural segments

[cAST](https://aclanthology.org/2025.findings-emnlp.430/) recursively splits large
AST nodes and merges smaller siblings under size constraints. It supports
structural segmentation, but its published algorithm is not proof that a
particular tokenizer budget is optimal. The
[BGE model card](https://huggingface.co/BAAI/bge-small-en-v1.5) lists 384 dimensions
and a 512-token sequence length for the current model. Engram's real fixture
already demonstrated identical vectors for different method tails after an
identical 8,192-character prefix. [moderate]

Downgrade: indirectness; the published evaluation is not Engram's task.
These three independent sources support bounded structural representation, not
a universal recall gain or an optimal segment size.

Recommendation: keep the complete method as the canonical source unit. Derive
multiple embedding passages with parent symbol identity, exact source ranges,
segment order and source-generation provenance. Count the **actual model's
tokens**, including context and special tokens. Prefer statement/block AST
boundaries; an oversized terminal node needs an explicitly marked continuation
fallback. Do not label every fragment a complete function. Cover the tail and
deduplicate/reassemble segments at their parent when presenting evidence.
[synthesis]

### 2. Improve candidate recall before fusion and reranking

[Qdrant](https://qdrant.tech/documentation/search/hybrid-queries/) separates
prefetch candidates from final output and supports multi-stage fusion.
[CodeRAG-Bench](https://arxiv.org/abs/2406.14497) reports difficulty retrieving
useful code with limited lexical overlap and downstream difficulty using
context. [Anthropic's contextual-retrieval study](https://www.anthropic.com/engineering/contextual-retrieval)
combines contextualized lexical/dense retrieval and reranking, but its reported
gains are vendor experiments on its own datasets. [moderate]

Downgrade: indirectness; the settings and gains are not transferable to Engram.
These sources support testing a staged pipeline, not claiming that adding a
reranker will fix missing candidates.

Recommendation: retrieve lexical and vector candidates more deeply than the
final result limit, fuse through existing RRF, optionally rerank with an actual
local scorer, then apply parent-level diversity and hydrate source evidence.
Compare graph lanes as candidate generators versus bounded expansion around
retrieved anchors. Do not ban a symbol such as `Page::map` by name; diagnose
why its lane promotes it for unrelated queries. MMR trades relevance against
diversity; test it separately from cross-encoder relevance ranking. [synthesis]

### 3. Add bounded, attributable context rather than ungrounded summaries

[SCIP](https://github.com/scip-code/scip) provides a language-neutral indexing
protocol for definitions, references and implementations. Anthropic's
[contextual method](https://www.anthropic.com/engineering/contextual-retrieval)
prepends compact chunk context before lexical/dense indexing.
[CoIR](https://arxiv.org/abs/2407.02883) covers multiple code-retrieval tasks,
illustrating that identifier lookup is not the whole workload. [moderate]

Downgrade: indirectness; compiler navigation facts and contextual document
retrieval do not by themselves establish better product-manager answers.

Recommendation: first build deterministic context from repository/path,
qualified symbol, signature, parent and resolved relevant imports. Keep each
fact linked to current source. Missing resolution should be explicit. Test
optional derived behavior summaries later; keep their source references,
generation/model/template versions and freshness independent from canonical
code. A summary may aid discovery but cannot replace the implementation used
to substantiate an answer. [synthesis]

### 4. Use layered typed configuration, with environment variables as overrides

[Figment](https://docs.rs/figment/latest/figment/) supports typed extraction,
profiles, file/environment providers and configuration provenance.
[config-rs](https://docs.rs/config/latest/config/) supports hierarchical defaults
and layered sources. [Twelve-Factor configuration guidance](https://12factor.net/config)
supports environment variables for deployment-varying configuration. Together
these support files/profiles for reproducible complex tuning and environment
overrides for deployment differences. [synthesis; moderate]

Downgrade: indirectness; none prescribes Engram's exact precedence or schema.
Twelve-Factor's preference for deployment environment variables does not mean
every algorithm setting must become a separate environment-only contract.

| Approach | Strength | Limitation | Recommendation |
| --- | --- | --- | --- |
| Environment only | Convenient in services and containers | Nested lane weights and reproducible parameter sweeps become awkward | Support overrides, not the sole interface |
| Typed JSON/TOML plus overrides | Reviewable, versionable, shared across transports | Requires documented merge rules and validation | Preferred contract |
| Named profiles | Simple default choice and reproducible tuning bundles | A name can hide changed behavior unless versioned | Add after measuring profiles |
| Runtime/admin tuning | Useful for controlled experiments | Requires authorization, concurrency semantics and rollback | Defer general hot reload; allow bounded query-level choices |

Prefer extending current serde/config loading first. Evaluate Figment if
profile/source provenance warrants a dependency; config-rs is another viable
loader. The public schema and merge behavior must remain independent of the
loader choice. No configuration library was added in this research.

### 5. Measure evidence quality independently from wiring and answer generation

[Elasticsearch's evaluation API](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/search-rank-eval)
uses manually rated queries/results for ranking metrics. CoIR provides
[code-specific retrieval tasks](https://arxiv.org/abs/2407.02883).
CodeRAG-Bench [separates retrieval and generation limitations](https://arxiv.org/abs/2406.14497).
These support a labeled, task-specific evaluation rather than counting any
vector-tagged hit as success. [synthesis; moderate]

Downgrade: indirectness; the final acceptance thresholds require Engram-specific
labels and user tasks. Measure candidate recall, final ranking and complete
evidence separately; a high first-hit metric cannot prove a multi-file answer.

## Additional testable cause: query representation

Engram's [FastEmbed adapter](../../adapters/sqlite/src/vector/fastembed_provider.rs)
currently prepends `query: `. BGE's
[model documentation](https://huggingface.co/BAAI/bge-small-en-v1.5) recommends its
own retrieval instruction or testing no instruction; it also cautions that raw
similarity values are not universal relevance thresholds. Treat this difference
as an ablation candidate, not a demonstrated cause of the retrieval failures.
[low: single model-specific source; impact unmeasured]

Compare the current prefix, the model's documented instruction and no prefix
on the same indexed passages. Query-only formatting can be evaluated without
regenerating unchanged passage embeddings, but must be recorded in the query
representation version and effective-config hash. Changes to passage context,
normalization, pooling, tokenizer or model require compatibility review and
usually new embeddings. [inference]

## Proposed configuration shape

One typed `CodeIntelligenceConfig` should extend `EngramConfig` and reuse
`RecallFusionConfig` for fusion; do not duplicate the fusion knobs. Separate
index representation from retrieval-time tuning and presentation budgets.
Environment/file resolution belongs in hosts/adapters; deterministic core code
receives validated values. Rust SDK, N-API, HTTP and stdio should expose the
same resolved behavior. [proposal]

Keep this as a code-intelligence profile: choosing code-oriented lane weights
must not silently change generic memory recall. Missing fields receive the
same typed defaults on every transport. Validate ranges, dependent budgets and
unknown keys in the new section; retain documented compatibility behavior for
legacy fusion files. A loader must distinguish omitted CLI options from CLI
defaults, otherwise default flag values can overwrite explicit file settings.
Profiles should carry a version so an upgrade does not silently redefine a
stored evaluation's configuration. [proposal]

Proposed precedence, lowest to highest:

1. Versioned built-in defaults and selected named profile.
2. An explicitly selected configuration file.
3. Namespaced environment overrides.
4. Explicit launch arguments or trusted SDK configuration fields.

Only allowlisted, bounded query settings may override that snapshot per request.
Scope/requester authorization and current-source checks are invariants, not
tuning knobs. Avoid reading the process environment on every query. Preserve
the existing `.engram/recall.json` fallback as a documented compatibility path;
define conflicts/migration instead of silently merging competing fusion values.
[proposal]

Example **future** fields, not commands that work today:

```toml
[code_intelligence]
profile = "balanced-v1"

[code_intelligence.index]
body_token_target = 384
context_token_budget = 64

[code_intelligence.retrieval]
candidates_per_lane = 50
result_limit = 10
```

Equivalent proposed deployment override:

```bash
ENGRAM_CODE__RETRIEVAL__CANDIDATES_PER_LANE=100
```

Explicit config-path selection is important because a scanned repository and
its persistent store are different roots. Profiles/config paths are deployment
inputs; do not load arbitrary scanned-repository configuration implicitly.
[proposal]

## Default candidates and change consequences

All new numeric defaults here are **starting hypotheses**, not tuned winners.
For BGE-small's 512-token window, 384 body tokens plus at most 64 context tokens
leave room for special tokens; enforce the actual tokenizer's total limit.

| Setting | Starting point | Tuning / consequence |
| --- | --- | --- |
| Source version | Current indexed generation; expose index age/revision | History only when explicitly requested and authorized; not a freshness bypass |
| Embedding model | Keep cached local BGE as baseline | Compare code-oriented alternatives on code-specific tasks; changing model requires compatible new vector space |
| Code/body target | 384 tokens | Sweep 256/384/448 with total-budget validation; rebuild derivative segments/embeddings |
| Context budget | At most 64 tokens | Sweep 0/32/64; affects passage embeddings and representation fingerprint |
| Lexical/vector depth | 50 per enabled lane | Sweep 20/50/100; query-time cost/recall trade-off |
| Returned parents | 10 | Query-time only; multiple segments of one method should not occupy every slot |
| RRF | Retain `k=60`, equal weights as control | Tune existing `k`/weights; do not copy another engine's defaults/formula blindly |
| Graph use | Bounded evidence expansion; compare current fused lanes as control | Ablate enabling/weights/expansion separately; zero weight need not avoid a lane's execution cost |
| Reranker | `none` for the existing no-reranker control | Explicit local cross-encoder experiment after wiring a scorer; MMR is a separate diversity experiment |
| Evidence budget | Token-budgeted, query-relevant ranges plus resolvable parent/source | Sweep presentation budget independently of vector input; preserve location and omitted-context reporting |
| Confidence/abstention | Evidence sufficiency status | Calibrate on labeled answerable/unanswerable questions; do not interpret normalized RRF as probability |

In particular, a “balanced” profile must not advertise cross-encoder quality
when construction falls back to no reranker. Record requested/effective strategy,
model availability, lane failures, candidate counts, truncation, config origin
and config hash in diagnostics. The exact diagnostic surface is a brief/spec
decision. [proposal]

Index-affecting changes need a representation fingerprint covering parser,
segmenter, tokenizer, model, passage context and normalization versions. Preserve
canonical source/memory data; rebuilding derived vectors must not delete the
shared database. Query-time knobs do not inherently require reindexing.
[proposal based on existing source/vector boundaries]

## Experiments before selecting defaults

Keep the existing ten questions and two unsupported-feature probes as a
regression set. Expand to independently labeled paraphrases, duplicate names,
late-method behavior, cross-file calls, configuration-dependent behavior,
language variants, historical/current revisions and insufficient-evidence cases.
Keep a held-out set so tuning on these ten questions cannot masquerade as
generalization. Include Engram and at least two structurally different repos.
[proposal]

Measure:

- Candidate Recall@50 before fusion; per-lane contribution and union recall.
- MRR/nDCG and expected-implementation discovery at final top 5/10.
- Required evidence coverage across files/symbols, not just one correct hit.
- Obsolete-source/doc hits, unauthorized hits, correct abstentions and unsupported claims.
- Token coverage of segments, tail behavior, exact source/parent reconstruction.
- Warm/cold latency distributions, phase timings, model calls and index size.
- Effective configuration parity across Rust, native TypeScript and both MCP transports.

Sequence the work: establish current-source/document eligibility and diagnostics;
test token segmentation/context and model query formatting; separate candidate
depth from final limits; compare fusion/graph/reranking; calibrate evidence
presentation and abstention. Use one controlled change at a time and paired
queries, then validate combined profiles on held-out data. Profile before
optimizing latency. [proposal]

Do not declare a percentage uplift, a universal model winner or a latency SLA
from the current self-evaluation. The next brief should set measurable targets
only after this expanded baseline is labeled.

## Known unknowns and limitations

- **Known unknown:** which lane/model/representation loses the eight missed
  questions before ranking. The existing output does not isolate this.
- **Known unknown:** query-prefix, context, segment and reranker effects in
  Engram; no comparative sweep was run during this research.
- **Known unknown:** optimal candidate depths/weights, a feasible latency budget
  on the user's hardware, and the cost of complete multi-file evidence.
- **Known unknown:** current-document identity/version contract and migration
  for historical chunks; code-generation guarantees do not automatically solve it.
- **Known unknown:** requirements for language/parser registrations and compiler
  adapters beyond the accepted RFC direction; SCIP is prior art, not a mandate.
- **Unknowable from retrieval alone:** a feature's absolute absence, behavior
  under unobserved configuration/runtime dispatch, or business intent. Return
  bounded evidence and uncertainty rather than asserting certainty from no hits.

This survey did not add a model, run new retrieval experiments, alter live
tuning, or implement a configuration schema. It is the research input for the
next brief.

## Source selection and confidence

Sources accessed 2026-10-14: published cAST/CoIR research, the CodeRAG-Bench and
Lost-in-the-Middle papers, official model/protocol/library documentation,
official search-engine documentation, Anthropic's own engineering report and
the local checkout/evaluation. Vendor results are not independent Engram trials.
The [Lost-in-the-Middle paper](https://arxiv.org/abs/2307.03172) concerns generator
context-position effects, not the embedding-prefix truncation demonstrated here;
it reinforces testing context presentation but does not justify a segment size.
[low: single source; indirectness]

Moderator check: model-card guidance surfaced the query-prefix ablation that
was absent from the initial question set. No additional numeric recommendation
is treated as validated. Facts cite primary sources; synthesis/proposals are
marked, with downgrade reasons stated for rated findings.
