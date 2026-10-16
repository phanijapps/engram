# Scoped quality review: baseline lineage and source history corrections

Verdict: Clean — ready to commit.

This is an independent agent review, not human signoff. Scope: the strict accepted-T0 baseline admission helper, identity freeze integration and synthetic evaluation tests; the current source-owned history correction, shared repository-contract transaction and focused tests, against the supplied T11 plan additions. Production files were reviewed read-only. No held-out labels, queries or results were inspected; no evaluation queries, real model inference or actual-store mutations were run.

## Findings

No unresolved findings in this scope.

The baseline validator admits only the exact pinned committed T0 receipt and final reviewed manifest. It verifies raw artifact bytes against that receipt and equality with its embedded report, then checks completion, binary/commit, corpus/revision/mode/split, ordered selected case inventory and authorization prerequisites. It verifies the actual baseline executable bytes. This preserves accepted measurements without a historical-manifest hash bypass or rerun. The helper is itself frozen, and the receipt plus all six raw baseline files are in identity(args), used by development, freeze and assessment drift checks.

The scoped history changes avoid global retention for unrelated KG writes/deletes. New code-contribution archive triggers are disjoint from existing document-head ownership and install under new names on initialized stores. Exact OLD JSON is retained for changed updates and deletes, while no-op updates do not archive. Generic file refresh/removal fences occur before cleanup; failed removal retains the discovered manifest hash for retry. Legacy source attribution and source-only validated repository-root preservation remain scoped to source ingestion. The canonical-chunk ID now covers immutable ownership fields without changing logical unit/generation/fingerprint identities or rewriting old rows.

Shared API/exposes projection operations validate repository, scope and endpoint identity, retain semantic history, preserve no-op observation provenance, and retract only the calling repository's ownership. SQLite encloses each operation in one IMMEDIATE transaction. The regression test now injects a real relationship UPDATE failure after preceding entity/history writes and proves exact entity, relationship and history rollback.

## Closed test gaps

- Synthetic baseline admission now rejects actual executable byte drift and selected inventory/order drift, in addition to raw report, receipt and reviewed-manifest drift.
- The freeze membership test explicitly checks the resolved receipt and all six raw report paths and independently mutates each file's bytes, preventing silent removal of one prerequisite from identity inventory.
- Repository-contract rollback now exercises a database failure after writes, rather than only rejecting identity before mutation.

## Independently executed verification

- `python3 -m unittest discover -s scripts/dev/tests -p 'test_code*eval*.py'`: 28 passed after the final freeze-membership test update; synthetic inputs only.
- `cargo test -p engram-ingest --offline --test contract_ingestion repository_contract`: 2 passed, including injected post-write rollback and shared retraction/history.
- `cargo test -p engram-store-sqlite --offline --lib knowledge::legacy_delete_tests`: 2 passed, covering unowned hard-delete lifetime, code-only exact archival, no-ops and repeated initialization.

This verdict closes the scoped implementation/test quality review. Whole-workspace gates, disposable-copy rollout rehearsal, frozen assessment and any product-quality claims remain the supervising loop's separate responsibilities; this report makes no claim that those pending gates have completed.
