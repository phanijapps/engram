# Source-preservation authoring review

- **Date:** 2026-10-14
- **Artifacts:** [spec](../spec.md), [plan](../plan.md)
- **Context:** RFC-0021 and the proposed next-step sequence were explicitly approved by the user. These are implementation-ready drafting artifacts; runtime work and AC completion have not begun.

Fresh Luna agents performed adversarial and security spec review. The fixed
bundled review models were known unavailable on this account from the RFC
review; the Luna fallback retains independent read-only review.

## Findings resolved

- Adversarial: snapshot exclusion needed a post-export/import negative test;
  AC5/AC10 and T6 now prove exclusion on the target's retrieval/index/model feeds.
- Adversarial: T7 needed concrete transport seams; it now names Rust MCP
  `main.rs`/`codegraph.rs`, TS HTTP `tools.ts`/`codegraph.ts`, and N-API provider
  and chunk modules, without pretending source-read already exists.
- Security: raw reads and role filtering do not grant model egress; code model
  feeds require separate source/destination authorization or exclude code.
- Security: snapshot reads require trusted effective exact scopes, with null
  distinct from wildcard and unresolved request dimensions denied.
- Security: import activation requires source-export and target-write approval,
  an explicit mapping and representable non-widening policy restrictions.

## Final verdicts

Both reviewers reported `Clean — ready to commit.` after reading the revisions.
This verdict is about the documents, not runtime behavior or a Git commit.
The ADRs record already-approved architectural decisions; the spec remains
Draft under the repository's status vocabulary.

## Verification

Contract, documentation, engine-neutrality and surface-parity scripts passed.
Local links, template residue, whitespace, AC-to-task references and task
ordering were checked. Source loss/runtime functionality is evidenced only by
the earlier audit; the new spec's complete delivery gates are unchecked until
implementation and live verification establish them.
