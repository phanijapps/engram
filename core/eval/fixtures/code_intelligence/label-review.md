# Independent blind code-intelligence label review

Reviewer: separate Codex agent `/root/code_eval_blind_review`, 2026-10-14. This is an **agent review, not human sign-off**. No retrieval, baseline-result, tuning, model-ranking or scorer outputs were read. Source labels and frozen production source were the only answer evidence.

Final label verdict after source-only recheck: **Clean — source labels ready for frozen evaluation.** Initial findings below are retained as review history and were resolved as recorded in the recheck section.

Reviewed all **119 entries**: 40 held-out answerables, 20 held-out negatives, 30 development answerables, 15 development negatives, original 10 answerables + 2 negatives, and 2 identifier controls. Original query text and required identities should remain unchanged.

## Mechanical checks

All 2,114 inventoried frozen files match their manifest SHA-256 values (Engram 1,299, Flask 236, Vite 579). All three label file digests matched the manifest at review time. Every labeled source hash, canonical declaration start/end and decisive range boundary is valid UTF-8 and in bounds. Existing `target/debug/examples/code_units` confirms one exact canonical parent range for every labeled required unit. Qualified names resolve after the existing intentional normalization: remove `fn `/`def `/`function ` prefixes and equate Python `.` with parser `::`. No required identity corrections are needed.

## Required corrections

**P1 — Required byte ranges omit essential claims in 20 Engram answerables.** A single line does not prove the full behavior. Examples: `engram-retrieval-policy` omits the private-owner condition; `engram-hierarchy` omits containment; `engram-unicode-snapshot` omits the too-small-bound failure; `engram-deny-path` omits case normalization. Returning those isolated lines would falsely pass evidence coverage despite not answering the questions. Apply `/tmp/engram_range_corrections.json`: complete relevant branches, not entire files. These corrections change only ground-truth evidence spans; no retrieval result was consulted.

**P1 — Metadata/source-byte equivalence lacks an essential entrypoint.** `engram-metadata-auth` currently labels only `authorize_code_source_access`; that helper proves policy behavior but not that metadata calls it or that source-byte authorization shares it. Add the two units in `/tmp/engram_metadata_extra_units.json`: `code_sources` (metadata explicitly invokes the helper) and `authorize_code_source_read` (source reads delegate to the same helper). Keep the existing shared helper unit and use its corrected expiry/owner branch.

**P2 — Three Vite evidence spans cut through the mechanism they claim.** `vite-h08` omits the `continue` that stops traversal through an accepted importer. `vite-h09` cuts the array/quoted-token lexer before complete acceptance branches. `vite-d09` begins after the import-specifier classification into default/namespace/named cases. Apply `/tmp/vite_range_corrections.json`.

**P2 — Authorization-denial negative class is missing.** All 35 development/held-out negatives cover absent features or irrelevant questions. Plan T0 explicitly names authorization denial. Add a separately frozen controlled private/expired source denial battery, with source admission and trusted launch requester recorded; do not spoof requester in a query. Existing original probes remain unchanged. The existing 20 held-out negatives are semantically valid; this is a coverage gap, not a claim that those questions are answerable.

## Semantic review

The labeled Engram source-preservation, fusion, configuration, filtering and cache predicates implement the questions; corrections above are needed to make decisive excerpt coverage meaningful. The original controls accurately locate locality default denial/prepare, donor filtering and backfill, generation recheck/insertion, input/batch bounds, HTTP/stdio scope guards, nested callable extraction, current cache authorization, shape/model failures, retry selection, and fixed grammar construction. Their full-body decisive spans are broad but valid. Original HTTP scope excerpt contains both scope/requester denial and missing trusted-context failure; original 12+identifier controls are accepted without changing identities.

All 25 Flask answerables have appropriate callable identities and complete decisive branches. The OPTIONS, before-request short circuit, async adaptation and uppercase custom-parser questions correctly label both essential methods. Nested generator/worker functions are retained inside the canonical outer callable, so their evidence spans remain mapped to the outer parent correctly. Debug/testing exception propagation is configurable as the source states; the rationale accounts for default behavior.

All 25 Vite answerables have appropriate production callable identities. Env precedence, strict FS authorization, public-directory containment, ancestor search, pending-transform invalidation, soft cache preference, HMR boundaries, package exports, glob conversion, inlining, worker asset registration, proxy matching and development helpers are implemented in the pinned files. The three span corrections above complete the evidence mechanism.

No answerable label relies on a test-only implementation or a speculative behavior summary. Grade 3 required-parent judgments are valid. The present collection does not exhaustively judge alternative/supporting retrieved parents; evaluator must report unjudged counts and may not imply complete graded relevance judgments.

## Negative verification boundaries

Negative labels are accepted as **no implemented production answer in the pinned indexed corpus**, never universal absence from extensions/dependencies/user applications. Keyword searches were corroboration, not the sole basis:

* Engram: inspected the deterministic parser map, scanner and ingestion flow (summary is `None`), source authorization, embedding path, neutral ports and adapter/host inventory. These paths parse/index/retrieve source; they do not implement collaborative edit CRDTs/cursors, indexed-code remote execution, JVM attachment, billing/payments, markets, APNs/video/FTP or replicated Raft storage. Incidental `draft` substring matches do not implement Raft. Original runtime-parser-plugin and per-method resolved-context/English-summary negatives are valid for this pinned implementation.
* Flask: inspected the complete 24-module production inventory, parsed all production Python imports with `ast`, and inspected routing/dispatch, cookie serialization, CLI registration and contexts. Built-in CLI serving/shell/routes are not database migrations; generic hooks/routing are not OTP/JWT/GraphQL/CSRF/ORM/job queue/metrics/mail/image processing. Signed-cookie `save_session` does not persist Redis data. WSGI serving/request response is not a WebSocket frame dispatcher. Flask-SQLAlchemy documentation mentions were excluded as extension guidance, not ORM implementation. External application extensions remain outside the stated absence scope.
* Vite: inspected production server/static/proxy/WebSocket transport, environment, plugin-container hooks, asset emission and manifest construction. Plugin hooks call handlers directly, asset emission uses the bundler output interface, HMR uses local channels/WebSocket, and manifest generation constructs bundler metadata; these adjacent real paths do not implement OAuth roles, Redis broadcasts, S3 upload/retry, KMS decrypt, syscall/container sandbox, application database rollback, cryptographic manifest validation, Brotli negotiation, IP quotas, Vault leases or protobuf/gRPC JSON bridges. Docker transport-timeout and client shipment-map mentions are unrelated. Third-party plugins/application behavior remain outside this production corpus boundary.

## Per-entry record

`Accepted after range correction` means required parent identity and semantic answer are accepted, with decisive ranges corrected as requested above. Negative accepted status is corpus-scoped and does not replace the missing authorization-denial battery.

| Entry | Blind source judgment |
| --- | --- |
| engram-retrieval-policy | Accepted after range correction |
| engram-hierarchy | Accepted after range correction |
| engram-unicode-snapshot | Accepted after range correction |
| engram-parse-state | Accepted after range correction |
| engram-source-policy-change | Accepted after range correction |
| engram-source-current | Accepted after range correction |
| engram-sharing-policy | Accepted after range correction |
| engram-weighted-fusion | Accepted after range correction |
| engram-storage-confinement | Accepted after range correction |
| engram-concept-override | Accepted after range correction |
| engram-metadata-auth | Add shared-helper entrypoint units and correct range |
| engram-scope-auth | Accepted after range correction |
| engram-generation-read | Accepted after range correction |
| engram-residual | Accepted after range correction |
| engram-source-attempt | Accepted after range correction |
| engram-source-persist | Accepted after range correction |
| engram-deny-path | Accepted after range correction |
| engram-filter-merge | Accepted after range correction |
| engram-markdown-fences | Accepted after range correction |
| engram-lexical-fresh | Accepted after range correction |
| engram-absent-crdt | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-payments | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-jvm | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-market | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-remote-runtime | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-watch-crdt | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-billing | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-mobile | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-pixels | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-ftp | Accepted; pinned production-corpus absence/irrelevance |
| engram-absent-replicated-log | Accepted; pinned production-corpus absence/irrelevance |
| original-locality | Accepted |
| original-snapshot-donor | Accepted |
| original-generation-race | Accepted |
| original-bounded-input | Accepted |
| original-transport-scope | Accepted |
| original-nested-body | Accepted |
| original-stale-cache | Accepted |
| original-invalid-vector | Accepted |
| original-retry | Accepted |
| original-new-language | Accepted |
| original-absence-parser | Accepted; pinned production-corpus absence/irrelevance |
| original-absence-silver | Accepted; pinned production-corpus absence/irrelevance |
| identifier-insert_current | Accepted |
| identifier-walk_declarations_with_receivers | Accepted |
| flask-h01 | Accepted |
| flask-h02 | Accepted |
| flask-h03 | Accepted |
| flask-h04 | Accepted |
| flask-h05 | Accepted |
| flask-h06 | Accepted |
| flask-h07 | Accepted |
| flask-h08 | Accepted |
| flask-h09 | Accepted |
| flask-h10 | Accepted |
| flask-h11 | Accepted |
| flask-h12 | Accepted |
| flask-h13 | Accepted |
| flask-h14 | Accepted |
| flask-h15 | Accepted |
| flask-d01 | Accepted |
| flask-d02 | Accepted |
| flask-d03 | Accepted |
| flask-d04 | Accepted |
| flask-d05 | Accepted |
| flask-d06 | Accepted |
| flask-d07 | Accepted |
| flask-d08 | Accepted |
| flask-d09 | Accepted |
| flask-d10 | Accepted |
| flask-hn01 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn02 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn03 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn04 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn05 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn06 | Accepted; pinned production-corpus absence/irrelevance |
| flask-hn07 | Accepted; pinned production-corpus absence/irrelevance |
| flask-dn01 | Accepted; pinned production-corpus absence/irrelevance |
| flask-dn02 | Accepted; pinned production-corpus absence/irrelevance |
| flask-dn03 | Accepted; pinned production-corpus absence/irrelevance |
| flask-dn04 | Accepted; pinned production-corpus absence/irrelevance |
| flask-dn05 | Accepted; pinned production-corpus absence/irrelevance |
| vite-h01 | Accepted |
| vite-h02 | Accepted |
| vite-h03 | Accepted |
| vite-h04 | Accepted |
| vite-h05 | Accepted |
| vite-h06 | Accepted |
| vite-h07 | Accepted |
| vite-h08 | Accepted after range correction |
| vite-h09 | Accepted after range correction |
| vite-h10 | Accepted |
| vite-h11 | Accepted |
| vite-h12 | Accepted |
| vite-h13 | Accepted |
| vite-h14 | Accepted |
| vite-h15 | Accepted |
| vite-d01 | Accepted |
| vite-d02 | Accepted |
| vite-d03 | Accepted |
| vite-d04 | Accepted |
| vite-d05 | Accepted |
| vite-d06 | Accepted |
| vite-d07 | Accepted |
| vite-d08 | Accepted |
| vite-d09 | Accepted after range correction |
| vite-d10 | Accepted |
| vite-hn01 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn02 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn03 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn04 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn05 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn06 | Accepted; pinned production-corpus absence/irrelevance |
| vite-hn07 | Accepted; pinned production-corpus absence/irrelevance |
| vite-dn01 | Accepted; pinned production-corpus absence/irrelevance |
| vite-dn02 | Accepted; pinned production-corpus absence/irrelevance |
| vite-dn03 | Accepted; pinned production-corpus absence/irrelevance |
| vite-dn04 | Accepted; pinned production-corpus absence/irrelevance |
| vite-dn05 | Accepted; pinned production-corpus absence/irrelevance |

## Source-only recheck, 2026-10-14

All 23 decisive-span corrections were applied exactly. The metadata authorization question now labels its shared authorization helper, metadata `code_sources` entrypoint, and `authorize_code_source_read` delegation. All **119 entries and 96 required-unit records** pass canonical parser range/name checks, source hashes, UTF-8 boundaries, and nonempty contained decisive ranges. Corpus inventories and three label hashes still match the manifest. Production corpus bytes and original query/required identities are unchanged. No retrieval/baseline/model/tuning outputs were inspected.

The separately defined authorization battery adds two source-read controls and two retrieval negatives without changing the 20 held-out absent-feature/irrelevance denominator. Its seeder writes a dedicated fixture only (create-new source files), admits private and expired sources in the exact trusted workspace under `eval-private-owner`, positively checks byte-exact private-owner access, and requires expired-owner denial. The real MCP runner launches outsider principal `engram-mcp` in that same trusted workspace; queries do not override principal/scope. These are genuine authorization-denial labels against retained canonical source, not absence labels mislabeled as permissions.

**Agent label review is complete. Human review remains pending; this review does not supply human approval.** The fixture/source labels may be frozen for evaluation. This is a source-label verdict, not a statement that the implementation or quality/performance gates passed.

Separate runner/evaluator follow-ups identified during the requested source-only runner inspection:

1. Check forbidden source sentinels across source-read error responses too; a policy-denial error with echoed private source must fail.
2. In evidence mode the isolated fixture contains only unauthorized code, so require empty `items` and `supportingUnits` as well as insufficiency; checking two literal sentinels alone misses partial unauthorized excerpts.
3. Bind the finalized authorization fixture hash in frozen manifest/runner validation. The runner currently records its input hash, which provides provenance but does not enforce predeclared identity.
4. Before final metric wiring, include supporting excerpt parts in required-range coverage as the spec requires; current `evaluate_code_query` accepts primary observations only. Do not treat source references without excerpt bytes as coverage.

Rechecked manifest SHA-256: `20af3f2bc47a161085ff9b867337935bc9017fffbf3b0ed41e1565983fd59eec`.

Authorization fixture SHA-256: `62c1eac29294cfc8fcef23b1002a852b7d94958b1a5fe9b42eefa291b2866625`.
