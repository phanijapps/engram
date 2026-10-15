# AC3 paired long-tail retrieval review

Verdict: Clean — ready to commit.

Independent agent review, read-only, scoped to packages/node/test/code-tail-retrieval.live.test.ts and /tmp/reliable-paired-code-tail-live.log. No held-out data was accessed; no inference, queries or store mutations were rerun. This closes the specific paired-ranking construction gap identified in /tmp/reliable-whole-spec-quality-preparation.md, not final whole-feature acceptance.

No unresolved findings.

The fixture uses two neutral parent names in one file and a shared 15,490-byte neutral body, with the cancellation/refund semantics only in distinct late calls. Natural-language queries do not name either parent. The real native provider explicitly selects FastEmbed BGE, vector enabled and vector-only fast retrieval. Assertions require populated vectors, drained backfill, no source failures, no lexical/graph-expansion contribution, disabled/no-call scorer, correct rank-one canonical parent for each query, a matching semantic tail excerpt beyond the parent's first 8,192 bytes, and exact UTF-8 source slices for every winning-parent excerpt. Distinct parent/segment identities are checked. Missing calibration deliberately remains insufficient/unvalidated inspection rather than claiming validated evidence quality.

The recorded opt-in live test actually ran and passed (one test, about 10.21 seconds), with 32 newly embedded code segments, zero reuse and zero remaining work. This is not a skipped model-free test. I independently reconstructed the fixture bytes and verified the logged source SHA256, all logged winning-parent range bounds/hash values, and each semantic tail beyond its parent's 8,192-byte prefix. The reconstructed file is 31,200 bytes with SHA256 83c611e3f27b9b7c8783202b72d357382d161906a41697a90d9a78837a025ef3, matching the live observations.

Combined with the existing actual_bge_tokenizer_covers_method_tail_without_truncation test, which changes a late tail after an identical 8,192-character prefix and asserts a changed real tail vector, the new competing-parent rank-one oracle supplies the previously missing AC3 distinction. Segment ID inequality alone is not treated as proof of vector inequality.

The test uses a disposable temporary store and removes it in finally. It imports no frozen labels, calibration or evaluation harness. Its public fixture/query strings cannot tune or consume the untouched held-out assessment. Production runtime/binary/config defaults are unchanged by this test-only addition. The parent reports package TypeScript checking passed; this reviewer did not independently rerun that gate.

Final frozen quality, actual-self rollout, operational p95, human review and final gate/identity reconciliation remain separate pending acceptance work. This report does not imply shipping or beta readiness.
