# Frozen reliable code-intelligence evaluation

This corpus measures the approved [spec](../../../../docs/specs/reliable-code-intelligence/spec.md),
not implementation completeness. Forty held-out answerable and twenty held-out
negative queries span Engram, Flask (Python) and Vite (TypeScript). Development
has thirty answerable and fifteen negative queries. Twelve original difficult
Engram probes and two exact-identifier controls are separate unchanged splits.
The authorization battery uses retained private/expired source with a fixed
outsider principal; it is separate from the absence-question denominator.

`manifest.json` binds corpus commits, all materialized file hashes, query order,
label hashes, the baseline executable and reference machine. Labels include
canonical callable spans and decisive UTF-8 byte ranges. An independent blind
agent verified source before tuning, corrected incomplete spans and recorded
[label-review.md](label-review.md). Human review remains pending. No label or
expected-answer artifact is sent to the MCP or installed in its indexed corpus.

Materialized source and stores live outside the working repository under
`~/.engram/evaluations/reliable-code-intelligence/`. Engram is an archive of
`98fda3c`, so editing evaluation artifacts here cannot contaminate its index.
Flask and Vite are separate pinned Git checkouts. Treat these roots as immutable;
the runner rejects changed source, added files, label drift or missing order.
Do not revise held-out labels after seeing scores.

Run `scripts/dev/code_intelligence_eval.py --help` for corpus arguments. Use
`--mode baseline --scan` once with the manifest-bound baseline binary; subsequent
splits reuse the same store. All sibling stores share a machine lock, and every
response has its own elapsed time. A single declared warm-up is excluded from
per-query latency. Raw JSON responses are retained with resumable checkpoints.
Candidate metrics are marked unavailable until an actual evaluation observer
provides the eligible pre-score order. Baseline search exposes no complete
candidate pool or subphase timings: those
metrics remain unavailable, not inferred from its top ten rendered headers.

The pure Rust metric evaluator lives in `core/eval/src/code_intelligence.rs`.
Candidate recall counts the first fifty distinct eligible implementation parents;
final recall and ranking use the first ten primary items. Supporting excerpts
can cover source bytes without recovering a primary parent. Overlapping excerpts
count once. Unvalidated inspection does not count as positive final evidence,
and negative questions never enter an artificial perfect-recall denominator.

Seed the isolated authorization corpus with the
`engram-ingest` example `code_eval_authorization`. It creates fixture files only
when they do not already exist, preserves canonical rows and verifies a positive
private-owner read. Then run `scripts/dev/code_eval_authorization.py`; it verifies
the manifest-bound fixture, real outsider source denials and absence of forbidden
source bytes in both successes and errors. In evidence mode it additionally
requires empty primary/supporting arrays and `insufficient_evidence`.
