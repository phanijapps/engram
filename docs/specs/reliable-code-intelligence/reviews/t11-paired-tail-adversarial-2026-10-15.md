# Paired long-tail retrieval — AC3 adversarial review

2026-10-15. Read-only review of `packages/node/test/code-tail-retrieval.live.test.ts`, accepted AC3 and the supplied actual-run log. No inference/model/evaluator rerun or held-out access. No production or frozen-input changes reviewed.

**Clean — ready to commit.**

The fixture places the two distinct cancellation/refund call tails beyond 8192 bytes in otherwise shared neutral 650-line function bodies. Parent names are neutral and neither question names them. Both parents compete in one actual scanned store, with actual BGE indexing required, no pending embedding work, and vector-only fast configuration. Diagnostics assert no lexical lane, no graph expansion, no scorer pairs/reranker and no source/reindex failures. Both natural-language questions must select the expected different parent at rank one and return its corresponding late-tail excerpt; every emitted excerpt must equal the exact source-byte slice. Uncalibrated output remains explicitly insufficient rather than becoming a quality/confidence claim.

Different segment IDs across different parents alone would be a weak oracle, but that is supplemental here: rank-one parent switching plus matching late ranges/text supplies the meaningful retrieval assertion. Together with the existing real-vector changed-tail/identical-prefix test and complete-range/token fixtures, this fills the competing-parent portion of AC3 without claiming frozen relevance results. The supplied live log shows one test executed and passed with 32 real embeddings; this reviewer did not rerun it. The opt-in environment/addon skip is explicit; ordinary suite success alone must not be cited as executing this live proof.
