# Remaining source-history correction — independent security review

Date: 2026-10-15
Verdict: **Clean — ready to commit.**

Scope: repository_contract.rs, CodeSourceRepository port/defaults and SQLite transaction wiring, scanner contract emission/retraction, disjoint code-owned canonical archive triggers and focused contract/legacy regressions. Read-only production review; no held-out inputs, models or retained-store writes.

- Companion canonical_code_* triggers require a code contribution and exclude document-head ownership. They preserve exact OLD JSON for changed UPDATE/DELETE without duplicating the existing head archive. Standalone unowned KG CRUD remains outside both archive predicates; the negative hard-delete regression is retained.
- The source-only repository-contract operations use an immediate transaction across validation, history and mutations. Stored/incoming scope, API kind, graphlessness, semantic name and exposes endpoint identities are checked; foreign root identity or endpoint collisions reject atomically. Validation failure rolls back any preceding history insertion.
- Incoming API references are restricted to exactly the named repository's Source reference, without injected URI/quote/location. Existing other repository references survive union and retraction. Retraction validates the stored edge and, when present, the exact repository root; an already-pruned root is allowed only on the cleanup path.
- Exact prior entity/relationship JSON is retained before semantic changes or removal. Metadata-only API changes are covered. Unchanged edge observations preserve existing provenance/timestamps rather than creating churn. Repeated completed retraction is idempotent and retains the shared API node for other repositories.
- Ownership guards reject attempts to modify independently managed canonical records; these narrow methods do not weaken generic KG mutation guards or create a general retention fallback. Their scope/repository arguments are trusted shared scanner inputs, not new wire authority overrides.
- Core defaults explicitly report unsupported. Existing scanner paths invoke the port when present and retain the prior helper path otherwise; no adapter type or SQL entered neutral contracts.

Reviewed focused regression sources covering exact history, metadata refresh, shared source-ref preservation, idempotent retraction, rejected endpoints/injected foreign references with rollback, no-op trigger behavior, dual-owner archive nonduplication and standalone KG lifetime. Parent reports 11 contract and 2 legacy tests passing; reviewer did not independently rerun tests or scanners. This signoff covers the construction correction only. Disposable-copy retention proof, incremental identity checks and release/quality gates remain separate.
