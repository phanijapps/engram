# T9 independent adversarial/spec review

Reviewer: `/root/code_eval_blind_review` (agent, not human). Read-only review of the T9 working-tree changes relative to `ba2db09`, against the accepted T9 plan and relevant AC1/AC6/AC8 requirements. No held-out labels or quality/tuning results were inspected. Reviewed the explicitly identified four-surface live artifact as transport/actual-model evidence; did not rerun heavy inference or alter production files.

## Verdict

Clean — ready to commit the T9 slice after the two readiness corrections recorded below. The focused regression rerun passed. This is a T9 slice review; calibration/quality gates and the full feature remain unshipped.

## Focused closure

Re-read both readiness corrections and their regressions. The local identity-only scorer remains unsupported through the default-false code-input declaration; the admitted local adapter opts in after construction. Preflight and all ranking locality/identity rechecks also enforce the declaration. The lexical-only fixture actually invokes the operation without vectors and returns items with Supported readiness; enabling the missing Vector lane changes readiness to Degraded.

Independently reran the full feature-enabled `code_evidence` integration target after these changes: **19 passed**. The parent-provided `/tmp/reliable-t9-review-fixes-tests.log` additionally records **76 library tests and 19 evidence tests passing**. No new findings in the focused re-review.

The parent subsequently reported the complete root pnpm test retry passed, including all 53 BFF tests, and all 174 MCP tests passed. Those broader gate outcomes are attributed to the parent, not independently rerun here. A post-fix native build/live retest was queued at review closure; no unobserved new live run is claimed by this report.

## Initial findings and corrections

1. **P2, corrected in source — scorer readiness accepted an identity without exact-input capability.** The initial `readiness::states` accepted any local scorer with a compatible nonempty identity, even when its query/document/pair inspectors retained the port's default `CapabilityUnsupported`. Every evidence call would then fail or fall back while capability reporting said Supported. The new compatible `supports_code_input()` declaration defaults false for generic/identity-only scorers; the admitted local adapter declares true only after construction. Both readiness and operation preflight require it, and the identity-only regression covers the old false-positive.

2. **P2, corrected in source — vector readiness was unconditional.** The initial embedding prerequisite required a matching code-vector handle even for lexical-only configuration, reporting Unsupported for an operation that can succeed without the vector lane. It also classified a missing enabled vector lane as Unsupported rather than the plan's Degraded missing-lane prerequisite. Readiness now establishes local embedding/representation authority independently, checks a compatible vector handle only when Vector is enabled, and classifies missing/incompatible configured lanes as Degraded. New lexical-only/no-vector and enabled-vector/no-vector cases exercise this behavior.

## Independently checked behavior

- All four surfaces call the same Rust `EngramProvider::code_evidence` operation; no scoring, source selection, evidence assembly or calibration logic is duplicated in TypeScript/transports.
- Native evidence handles own an `Arc<EngramProvider>` and a decoded validated trusted `CodeSourceContext`; strict per-query JSON cannot replace scope/requester/model/calibration. Host context is copied at handle construction, so later mutation of the caller's JS object does not mutate authority.
- HTTP `code op=evidence` has a strict allowlist and requires trusted launch context. It delegates through typed generated query/packet contracts to the bound native operation. Without trusted context it refuses dispatch and reports evidence unsupported.
- Stdio removes only the operation discriminator and decodes the same strict Rust query DTO. Scope/requester come from launch context. Query/identifier byte limits and actual tokenizer admission therefore reach embedded, native and both MCP transports through shared Rust behavior.
- The explicit calibration path is trusted startup configuration, loaded once through a focused bounded regular-file reader. Malformed/missing/oversized/FIFO/symlink selections fail explicitly; runtime query callers cannot install or replace the artifact. SQLite, pgvector and Surreal recipes all install the immutable artifact. Request-time identity matching remains in the shared operation.
- Capability reporting distinguishes source/embedding/retrieval prerequisites from actual local scorer capability and does not certify calibration or quality. Unsupported backend authority remains explicit rather than producing a successful empty operation.
- Native and stdio builds propagate the local scorer feature. The native provider's Arc ownership change preserves existing handle forwarding; existing recall paths remain unchanged.
- `docs/research/reliable-code-intelligence-surface-live-2026-10-15.json` records the same Flask query over Rust/native/stdio/HTTP with real local scorer execution, 42 successful pairs, matching typed packets excluding timing, schema validation and explicit unvalidated/insufficient status. Its f32 comparison treatment is declared: the Rust diagnostic wrapper promotes f32 values through json!, while direct native/stdio/HTTP item JSON agrees. `featureShipped` remains false.

## Independently run checks

- Initial `cargo test -p engram-integration --offline --features sqlite,fastembed,cross-encoder-rerank --test code_evidence`: **17 passed**.
- `pnpm --filter @engram/node exec vitest run test/provider.test.ts`: **11 passed**.
- `pnpm --filter @engram/runtime exec vitest run test/mcp.test.ts`: **16 passed**.
- `cargo test -p engram-mcp --offline --features fastembed,cross-encoder-rerank strict_evidence_wire_rejects_authority_and_model_fields_before_provider_use`: **1 passed**.

The first root TypeScript gate had transient BFF 503 failures. See the focused closure above for the parent's later successful full retry. This review independently ran the scoped Rust/native-transport/HTTP tests listed here, not the whole root gate.
