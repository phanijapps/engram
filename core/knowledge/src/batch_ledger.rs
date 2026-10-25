//! Source batch ledger port — durable per-source supersession bookkeeping.
//!
//! Sourced distill batches ([`SourceBatchLedger`] consumers, e.g. the batch
//! ingest composition) need a durable record of *what each batch wrote* per
//! source identity, because fact (memory) record ids are engine-assigned and
//! never seen by the caller. The ledger binds `(scope, source_id, batch_key)`
//! to the written record ids; supersession reads it as the declared prior set
//! and matches records by ledger id plus source-ref membership — never by
//! idempotency-key equality and never by entity name.
//!
//! This is a neutral port (ADR-0022): types and traits only, no SQL, no engine
//! names. Storage lives in engine adapters. Defaults follow the additive-port
//! convention (`list_*` default empty; mutations default not-supported), so an
//! engine without an implementation degenerates honestly: supersession finds
//! no prior rows and records nothing without silently claiming success.

use async_trait::async_trait;
use engram_domain::{Scope, Timestamp};
use engram_runtime::{CoreError, CoreResult};

/// The kind of record a ledger row binds to. `record_id` is opaque to the
/// ledger (entity/relationship ids verbatim; engine-assigned memory ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceBatchRecordKind {
    Entity,
    Relationship,
    MemoryRecord,
}

/// One ledger row: a single record written by one sourced batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBatchRecord {
    /// The caller-declared, revision-stable source identity.
    pub source_id: String,
    /// The idempotency key of the batch that wrote the record.
    pub batch_key: String,
    /// Which store the record lives in.
    pub kind: SourceBatchRecordKind,
    /// The opaque record id (entity/relationship id, or engine-assigned memory id).
    pub record_id: String,
    /// Set when a later batch of the same source superseded this record.
    pub superseded_at: Option<Timestamp>,
}

/// Durable per-source batch ledger port.
///
/// Every operation is scope-bounded: rows are keyed by the scope discriminator
/// columns, so one scope can never read or mutate another scope's ledger.
///
/// Concurrency stance: the ledger rides a single store connection; the
/// read-modify-write ref-union supersession that consumes it assumes a
/// per-scope single writer (the established contract-entity precedent).
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait SourceBatchLedger: Send + Sync {
    /// Lists the source's **not-yet-superseded** ledger rows within `scope`
    /// (all batch keys). Default: empty (unsupported — supersession no-ops).
    async fn list_source_batch_records(
        &self,
        _scope: &Scope,
        _source_id: &str,
    ) -> CoreResult<Vec<SourceBatchRecord>> {
        Ok(Vec::new())
    }

    /// Records that `batch_key` wrote `record_ids` for `source_id` (upsert by
    /// the ledger's unique row key; a re-record clears `superseded_at` so a
    /// re-asserted record becomes the current batch's row again). Default:
    /// not supported — surfaced, never silently dropped.
    async fn record_source_batch_writes(
        &self,
        _scope: &Scope,
        _source_id: &str,
        _batch_key: &str,
        _kind: SourceBatchRecordKind,
        _record_ids: &[String],
    ) -> CoreResult<()> {
        Err(CoreError::Adapter {
            adapter: "source_batch_ledger".to_owned(),
            message: "recording source batch writes is not supported".to_owned(),
        })
    }

    /// Marks one ledger row superseded (sets `superseded_at`). Default: not
    /// supported.
    async fn mark_source_batch_superseded(
        &self,
        _scope: &Scope,
        _source_id: &str,
        _batch_key: &str,
        _kind: SourceBatchRecordKind,
        _record_id: &str,
    ) -> CoreResult<()> {
        Err(CoreError::Adapter {
            adapter: "source_batch_ledger".to_owned(),
            message: "marking source batch records superseded is not supported".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Default-behavior and type-shape tests for the additive ledger port: the
    //! `list_*` default is empty (supersession degenerates to a no-op), the
    //! mutation defaults are honest not-supported errors, and the additive
    //! `KnowledgeRepository` methods follow the same convention. (Serde shape
    //! is exercised in the store adapter's tests, where serde_json exists.)

    use super::*;
    use crate::KnowledgeRepository;
    use engram_domain::Scope;

    fn scope() -> Scope {
        Scope {
            tenant: "tenant-ledger".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    struct NoLedger;

    #[async_trait]
    impl SourceBatchLedger for NoLedger {}

    /// A minimal repository stub: only the required methods are implemented,
    /// so the default bodies of the four additive methods are what executes.
    struct NoopRepository;

    #[async_trait]
    impl KnowledgeRepository for NoopRepository {
        async fn put_source(
            &self,
            _source: engram_domain::KnowledgeSource,
        ) -> CoreResult<engram_domain::KnowledgeSource> {
            unreachable!("stub")
        }

        async fn put_document(
            &self,
            _document: engram_domain::SourceDocument,
        ) -> CoreResult<engram_domain::SourceDocument> {
            unreachable!("stub")
        }

        async fn put_chunk(
            &self,
            _chunk: engram_domain::KnowledgeChunk,
        ) -> CoreResult<engram_domain::KnowledgeChunk> {
            unreachable!("stub")
        }

        async fn get_chunk(
            &self,
            _id: &engram_domain::ChunkId,
            _scope: &Scope,
        ) -> CoreResult<Option<engram_domain::KnowledgeChunk>> {
            Ok(None)
        }
    }

    #[test]
    fn ledger_defaults_degenerate_honestly() {
        let ledger = NoLedger;
        let listed = futures_block_on(ledger.list_source_batch_records(&scope(), "docs/x.md"));
        assert!(matches!(listed, Ok(rows) if rows.is_empty()));
        let recorded = futures_block_on(ledger.record_source_batch_writes(
            &scope(),
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::Entity,
            &["e-1".to_owned()],
        ));
        assert!(recorded.is_err(), "record default must be not-supported");
        let marked = futures_block_on(ledger.mark_source_batch_superseded(
            &scope(),
            "docs/x.md",
            "batch-1",
            SourceBatchRecordKind::Entity,
            "e-1",
        ));
        assert!(marked.is_err(), "mark default must be not-supported");
    }

    #[test]
    fn repository_additive_defaults() {
        let repo = NoopRepository;
        let by_ref = futures_block_on(repo.list_entities_by_source_ref("docs/x.md", &scope()));
        assert!(matches!(by_ref, Ok(rows) if rows.is_empty()));
        let rels = futures_block_on(repo.list_relationships_by_source_ref("docs/x.md", &scope()));
        assert!(matches!(rels, Ok(rows) if rows.is_empty()));
        assert!(
            futures_block_on(repo.restore_entity(&engram_domain::EntityId::from("e-1"), &scope()))
                .is_err(),
            "restore_entity default must be not-supported"
        );
        assert!(
            futures_block_on(
                repo.restore_relationship(&engram_domain::RelationshipId::from("r-1"), &scope())
            )
            .is_err(),
            "restore_relationship default must be not-supported"
        );
    }

    #[test]
    fn ledger_record_type_shape() {
        // Construction + equality is the type-shape check; serde round-trip
        // lives in the store adapter tests (this crate has no serde_json dep).
        let record = SourceBatchRecord {
            source_id: "docs/x.md".to_owned(),
            batch_key: "doc-x@abc123".to_owned(),
            kind: SourceBatchRecordKind::MemoryRecord,
            record_id: "memory-1".to_owned(),
            superseded_at: None,
        };
        assert_eq!(record.kind, SourceBatchRecordKind::MemoryRecord);
        assert_ne!(record.kind, SourceBatchRecordKind::Entity);
        let superseded = SourceBatchRecord {
            superseded_at: Some(chrono::Utc::now()),
            ..record
        };
        assert_ne!(superseded, {
            SourceBatchRecord {
                superseded_at: None,
                ..superseded.clone()
            }
        });
    }

    /// Drives a future on the current thread without an async runtime (the
    /// port crate depends on async-trait but not on an executor).
    fn futures_block_on<F: std::future::Future>(f: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut fut = std::pin::pin!(f);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }
}
