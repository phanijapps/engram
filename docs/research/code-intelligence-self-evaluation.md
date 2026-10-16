# Code intelligence: adversarial self-evaluation

Date: 2026-10-14. Evaluated commit: `98fda3c6d1`.
Store: `~/.engram/mem-alpha-self`; vectors enabled, cached local
BGE-small-en-v1.5. [Recorded prompts and responses](code-intelligence-self-evaluation-2026-10-14.json).

**Verdict: embedding and source-safety plumbing works; reliable natural-language
implementation discovery is not established and failed this diagnostic.**
The earlier four probes established vector wiring, not answer quality.

## Method and fresh checks

Required implementation anchors were selected from source before retrieval.
Ten difficult implementation questions requested ten results each through the
real release MCP server. Documentation, specifications, review verdicts and
tests did not count as substitutes for implementation. Two additional questions
deliberately presupposed features that are not implemented. Three failed
questions were also sent through the recommended general `recall` tool, and
two exact-identifier searches checked whether the implementation was indexed.

Discovery means locating the required source symbols/files; it does not mean
the returned excerpt contains enough evidence to answer. The constructor
`TreeSitterChunker::new` renders as `pub fn new()` in its unique implementation
file; its rank was verified manually rather than counted as a missing symbol.

| Fresh check | Result |
| --- | --- |
| Incremental self scan | 999 unchanged; zero ingested/errors/admission failures |
| Backfill | Zero embedded, zero pending; code/vector capabilities supported |
| Source-selection regression suite | 19 passed, zero failed; 2.41 seconds |
| Live forged-scope reindex | JSON-RPC `-32602`: trusted launch scope required |
| Exact MCP source reads | Seven implementation files matched checkout bytes |
| Disposable long-function fixture | Two additional exact source reads; complete callable tails retained |

The 19 regressions exercise external-provider exclusion, snapshot/hash donor
exclusion, generation and expiry changes during inference, cached access
rechecks, scope/private-source denial, model/insertion failures, retry,
deduplication and bounded large inputs. Wrong-vector dimensions, NaN and model
space checks were source-inspected in this evaluation; this run did not inject
those malformed provider outputs. The previous 1,070 Rust / 206 TypeScript
workspace results are historical evidence, not newly rerun totals.

## Difficult questions and implementation discovery

The JSON artifact contains each full question, required anchors, ranked
headers, returned excerpts and elapsed time. Below, “miss” means the required
implementation was absent from the requested top ten, even if useful docs or
tests appeared.

| Question | Expected implementation | Result |
| --- | --- | --- |
| Can changing providers send source to a remote model? | `prepare`, `EmbeddingProvider::is_local` | Miss |
| Can a snapshot donate a same-hash method vector? | `eligible_refs`, `backfill` | Miss |
| What if a file changes during inference? | `still_current`, `insert_current` | Miss |
| Does the final statement of a long method affect its vector? | `prepare`, `backfill` | Miss |
| Can maintenance forge scope/requester on HTTP or stdio? | HTTP `registerTools`, stdio `reindex` | Located at ranks 3 and 2; HTTP guard outside excerpt |
| How is a nested callable's outer body retained? | `walk_declarations_with_receivers` | Miss; parser test appeared instead |
| Can warmed vector caches bypass a new private/expiry policy? | `KnowledgeVectorResolver::resolve`, `source_record_access` | Miss |
| What about NaN, wrong dimensions or model space? | `insert_current`, `backfill` | Miss |
| Does a failed backfill retry completed insertions? | `backfill`, `insert_current` | Miss |
| Where must a new language grammar be registered? | `TreeSitterChunker::new` | Located at rank 4 |

**Two of ten questions located the required implementation anchors; three of
18 required anchors appeared.** This is a strict discovery diagnostic, not an
LLM answer-accuracy score. The HTTP excerpt alone did not establish its guard.
Direct source reads were required for authoritative answers and were not
retroactively counted as search success.

General `recall` also missed the required implementation for locality,
generation-race and nested-body questions (zero of three). Exact searches for
`insert_current` and `walk_declarations_with_receivers` both located their
correct function at rank one. Those top hits came from graph/associative/
community lanes, not evidence of vector-only success. The implementation is
indexed; broad discovery is the weak path.

`Page::map` appeared in all 12 primary searches despite being unrelated.
Documentation and low-information graph results competed with code evidence.
The primary searches took 10.739–16.291 seconds, median 10.817 seconds, after
capability/scan/backfill calls. These are local diagnostic timings, not a
controlled performance comparison; some follow-up work overlapped the final
probes. There was no vectors-off baseline, so no quality improvement percentage
is justified.

## A demonstrated vector blind spot

In a disposable code-only store, two TypeScript files defined the same
`processRecord` function with an identical long prefix. Their final behavior
differed: one cancelled a subscription and revoked premium access; the other
refunded a merchant charge and reversed payment.

Both exact files and complete callable bodies were preserved. The callable
texts contained 13,090 and 13,083 characters. Their first 8,192 characters were
identical, so the bounded model inputs were identical. Reading both persisted
vectors through `VectorIndex::vector_for_target` found **384 dimensions,
bit-for-bit equality, maximum difference zero**. Distinct full-content hashes
did not cause distinct vectors. Backfill finished and repeated with zero work.

This is a constructed boundary case, not an estimate of how often real queries
fail. It proves that complete AST source preservation does not imply complete
vector coverage. The model token window can shorten effective coverage further.
Lexical retrieval might still distinguish the tails; this test makes no claim
that all fused retrieval must fail.

## Freshness and unsupported-feature probes

The locality search returned an obsolete `docs/backlog.md` chunk containing
`[ast-outer-body-loss] VERIFIED`, describing the now-fixed body-loss bug. The
current checkout no longer contains that text. The retained knowledge store
has four document versions of the same AST-audit heading; the obsolete chunk
is `chunk-b1d59be3ebf06d0d8fcae2249c73fbe84b147fdea262d83364bf842a569e649b`,
with provenance from `22013a1b04`. An incremental scan with zero errors and an
idle reindex did not prevent that older document version from being returned.
This observation concerns document candidates; it does not demonstrate a
failure of the current code-generation authorization checks.

Two questions asked for a runtime parser plugin registry and for a scan step
that attaches resolved imports, constructor fields and English summaries to
every method. Neither feature is implemented. Source inspection shows a
private, built-in grammar map in `TreeSitterChunker::new`; the ingestor leaves
entity `summary` unset. RFC-0021 proposes later parser/context work. Search
returned plans or unrelated results, not an authoritative negative answer.
These probes are excluded from the ten-question denominator; no automated
abstention claim is made.

## Source-grounded answers and next work

The source checks confirm these answers even where retrieval failed:

- Raw code inference requires a local provider plus current manifest, scope,
  policy, role and generation checks. External eligibility stays separate.
- Snapshot and unverified legacy code are excluded; eligible same-hash donors
  and targets are rechecked before reuse/insertion.
- Generation/attempt/content are rechecked after inference. Canonical source
  ownership also gates retrieval. This is not cross-store atomic insertion.
- Existing vector IDs are skipped on retry; failures propagate and prior
  successful insertions remain. Limit is 1–1024, model groups are at most 64,
  and each input is capped at 8,192 characters.
- Callable AST nodes retain their full body; structural containers use small
  context and separate child extraction. New grammars currently require a
  constructor/node-map/dependency change and rebuilding.

Priority fixes are to select current document versions for normal discovery
while keeping historical access explicit; embed linked, token-budgeted AST
segments that cover a method's tail; and improve code-evidence ranking and
follow-up source hydration so irrelevant graph hubs and plans do not replace
implementation. Re-run the same frozen questions after each change, alongside
source authorization and snapshot exclusions. Parser registration, dependency
hydration and Silver summaries remain separately scoped work.

## Reproduction

```bash
cargo test -p engram-conformance --test code_source_selection
python3 scripts/dev/mcp_driver.py --storage ~/.engram/mem-alpha-self \
  --project mem-alpha-self --wait 12 \
  call code '{"op":"search","query":"A source file changes while the embedding model is running. What prevents the result being inserted for the obsolete indexing attempt?","limit":10,"diagnostics":true}'
```

Use the recorded JSON's `cases[].query` for the remaining probes. `code` is the
core-profile wrapper for granular `search`. The evaluation used sequential
JSON-RPC requests with elapsed time measured around each response; the current
stock driver queues calls and should not be used for multi-call latency claims.
Machine-local raw evidence and drivers are in `/tmp/engram-hard-eval/`,
`/tmp/engram_hard_eval.py`, and `/tmp/engram-tail-eval-mcqhphz0/`. The JSON artifact
preserves prompts, responses and fixture counts when those temporary files expire.
