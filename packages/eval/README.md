# @engram/eval

Future home for TypeScript evaluation fixture helpers and CLI wrappers.

Evaluation behavior should stay aligned with `docs/specs/` and
`contracts/v1/examples/`.

The Rust `engram-eval` crate owns fixture execution, report summaries, and
architecture capability coverage. TypeScript helpers should load or display
those reports instead of re-implementing recall, leakage, ranking, hierarchy,
taxonomy, belief, consolidation, or adapter-readiness checks.
