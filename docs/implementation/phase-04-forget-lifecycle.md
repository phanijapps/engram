# Phase 04 Plan: Forget Lifecycle

The lifecycle slice is implemented in `engram-store-memory/src/forget.rs` and
delegated from `service.rs`. Tests cover delete, redact, tombstone, archive,
policy denial, and cross-tenant isolation.
