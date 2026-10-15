# T11 focused observability and operator-doc review

Reviewer: `/root/code_eval_blind_review` (agent, not human). Read-only review of the focused provider observability correction, its strengthened supplied-handle regression, unchanged conformance assertion, and the new code-evidence section of `mcp/engram-mcp/HOW-TO.md`. No held-out labels, queries, results or real model runs were accessed.

## Verdict

Clean — ready to commit.

This verdict covers only the focused T11 observability correction and operator documentation. No blocking findings; pending quality gates and whole-feature shipment remain outside this signoff.

## Observability correction

- `ProviderObservability` is a focused neutral decorator. It awaits the existing adapter's `diagnostics()` and propagates its error unchanged, then replaces only `snapshot.capabilities` with the finalized immutable provider report. Record counts, embedding configuration, schema/adapter versions and retrieval diagnostics remain adapter-supplied.
- The builder installs the decorator after computing the actual code-evidence/code-scorer readiness, so diagnostics cannot retain the earlier bootstrap capability snapshot. The provider owns an immutable report; the decorator neither recomputes readiness nor introduces adapter/engine dependencies, query behavior or new storage reads.
- Missing observability handles remain absent. The existing concrete adapter conformance assertion is unchanged: diagnostic capabilities must equal the provider's full report. The strengthened custom-handle test also checks retained configuration/version metadata, rather than weakening comparison to selected capability fields.
- The plan records the construction correction and its required regression before implementation.

## Operator documentation

The new section matches the current Rust-owned profile defaults, strict query DTO casing, environment nesting/precedence, model identities and provisioning entry point, trusted-host authority boundary, failure policies, optional startup calibration, and separate packet/configuration diagnostics. It accurately distinguishes scorer/operation availability from calibration and quality acceptance and preserves the insufficiency/score-space semantics. Reindex guidance preserves canonical data/history and existing namespaces instead of deleting the store.

The parent applied the optional wording improvement: `--from-cache` explicitly names an existing Hugging Face hub cache root with `models--…/snapshots/<revision>/…`, distinct from the verified provisioned destination selected by `ENGRAM_MODEL_CACHE`.

## Independent checks

- `cargo test -p engram-integration --offline --features sqlite --lib builder_attaches_observability_handle`: **1 passed**.
- `cargo test -p engram-conformance --offline --lib provider_exposes_observability`: **1 passed**, exercising the unchanged full-report equality assertion.

The parent reports the full workspace retry passed with 1,181 tests, 10 ignored and zero failures, and five native actual-model smoke tests passed. Those broader results were not independently rerun in this focused review. This report does not sign off unrun held-out quality or the full feature.
