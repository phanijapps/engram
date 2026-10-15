# Code candidate ownership lookup, 2026-10-15

The frozen Flask development store and the question “register a URL rule and
endpoint on the Flask application” exposed a per-hit whole-store authorization
scan. Final output limit was ten; independent candidate limits were 50 and 20.
This is a component/candidate-stage measurement, not final evidence latency,
quality, calibration, or the 60-request p95 gate.

Before: the real candidate probe exceeded 120 seconds. Instrumented repetitions
exceeded 45 seconds, having resolved only eleven lexical hits. Provider startup
was 1.42–1.55 seconds, source authority about 9 milliseconds and raw Tantivy search
about 0.73 milliseconds for 50 hits. The first lexical target's full
`source_record_access` took 3.53 seconds; subsequent hits repeated that work.
The exact read-only contribution SQL fetched 300,823 rows in 2.324 seconds.
Manifest/head reads took approximately 5.79/1.61 milliseconds. This attributes
the delay to repeated whole-store ownership work, rather than model inference.

The lexical resolver now obtains all ownership kinds and independent owners for
its one target using bound target-ID predicates. It retains the same attempt,
head, scope, expiry, allowed-use and private-actor checks. A target-bound return
type prevents a caller from treating an omitted ID as unrestricted. Full-store
callers retain the original access-map API. Equivalence fixtures compare every
permission and policy for private/shared/legacy/snapshot/unmanaged targets,
relationships, generic document ownership and same-ID/different-kind records.

After: on the same store/question/candidate caps, the actual indexed candidate
stage completed in 1.2746 seconds at 50 per lane (50 vector + 50 lexical hits;
93 unique fused segments, 85 parents, 8 expansion segments) and 0.9496 seconds
at 20 (20 + 20; 42 segments, 35 parents, 7 expansion segments). The recorded
lexical phases were 1.1365 and 0.8650 seconds. This is a timeout lower-bound
improvement, not a precise end-to-end speedup ratio. The local-asset rollout also
changed the passage identity and fixed CPU inference to one intra-op thread;
therefore these timings must not be presented as an isolated model comparison.
A separate instrumented run of the same target-filtered authorization path had
approximately 5-millisecond access checks and a 0.906-second lexical phase.

The offline BGE namespace was reindexed explicitly before the full candidate
proof. Existing namespaces remain; canonical source, document, chunk, graph,
ownership and history table digests match before and after. All returned
candidate source hashes match frozen file bytes; segment IDs are unique and
contributing RRF trace scores reconstruct the result. Timers were removed from
production code. Raw candidate results and reindex digests are retained in
`docs/research/reliable-code-intelligence-t6-live-2026-10-15.json` and
`docs/research/reliable-code-intelligence-offline-reindex-2026-10-15.json`.
