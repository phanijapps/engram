//! Backend-neutral best-effort batch ingest port (engram-host-sdk brief, S3).
//!
//! [`BatchIngest`] writes a **semantic batch** — an optional episode
//! (source/documents/chunks), facts (memory records), graph entities, graph
//! relationships, evidence links, and embedding references — across the
//! relevant stores through one facade-level operation carrying a single batch
//! [`BatchIngestRequest::idempotency_key`].
//!
//! Because the backing stores live in **separate databases** with no
//! cross-store transaction, the operation is **best-effort, not ACID**: each
//! step writes in its own per-store transaction, in a fixed order
//! ([`ALL_STEPS`]), and on partial failure the host receives a per-step
//! [`BatchOutcome`] naming exactly which steps [`StepStatus::Succeeded`], were
//! [`StepStatus::Deduplicated`], were [`StepStatus::Skipped`], or
//! [`StepStatus::Failed`] (with a typed [`CoreError`]). There is **no rollback
//! of already-succeeded steps** — that would be a false claim of atomicity.
//!
//! The guarantee is surfaced explicitly as
//! [`TransactionGuarantee::BestEffort`] — never overclaimed as atomic.
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds no
//! SQL (enforced by `.codex/hooks/check-engine-neutrality.sh`). The SQLite
//! implementation lives in the adapters layer (`engram-conformance`).

use async_trait::async_trait;
use engram_domain::{
    EmbeddingRef, EvidenceRef, KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship,
    KnowledgeSource, MemoryRecord, Scope, SourceDocument,
};
use engram_runtime::{CoreError, CoreResult};

/// The transactional guarantee a [`BatchIngest`] backend provides.
///
/// The SQLite backend is [`BestEffort`](Self::BestEffort): separate store files
/// cannot share a transaction, so a step failure does not roll back earlier
/// steps. `Atomic` is reserved for a future single-connection backend that can
/// offer true cross-store ACID; no v1 backend returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionGuarantee {
    /// Each step writes in its own per-store transaction; a later step's
    /// failure does not roll back earlier steps. Succeeded steps stay landed.
    BestEffort,
    /// Reserved: a future single-connection backend offering true cross-store
    /// ACID. No v1 backend returns this.
    Atomic,
}

/// One logical write phase of a batch, in the fixed order a backend executes
/// them ([`ALL_STEPS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchStep {
    /// Optional `source` + `documents` + `chunks` (the episode).
    Episode,
    /// Memory records (`facts`).
    Facts,
    /// Graph entities.
    Entities,
    /// Graph relationships.
    Relationships,
    /// Evidence links (`EvidenceRef`). Skipped in v1 — evidence is embedded in
    /// record provenance.
    Evidence,
    /// Embedding references (`EmbeddingRef`). Skipped in v1 — vector storage is
    /// a VectorIndex follow-up.
    Embeddings,
}

/// The fixed execution order of every batch step. A [`BatchOutcome`] reports
/// one [`StepOutcome`] per step in this order.
pub const ALL_STEPS: [BatchStep; 6] = [
    BatchStep::Episode,
    BatchStep::Facts,
    BatchStep::Entities,
    BatchStep::Relationships,
    BatchStep::Evidence,
    BatchStep::Embeddings,
];

/// The per-step outcome status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    /// The step wrote its payload (or a non-empty payload landed).
    Succeeded,
    /// The step's payload was already present under the same key — the store
    /// deduplicated the write (key-based dedup, e.g. the Facts step's memory
    /// idempotency).
    Deduplicated,
    /// The step had nothing to write (empty payload) or is intentionally not
    /// wired in v1 (Evidence/Embeddings).
    Skipped,
    /// The step failed. The typed error is carried in [`StepOutcome::error`];
    /// the batch continues with the remaining steps (best-effort).
    Failed,
}

/// The outcome of a single batch step. The `error` is typed
/// ([`CoreError`], not a string) and present only when `status == Failed`.
///
/// When multiple records fail *within* one step, only the **first** error is
/// reported here; later errors in the same step are not surfaced. A caller
/// debugging a multi-record failure should treat this as one representative
/// error, not the complete failure set.
#[derive(Debug)]
pub struct StepOutcome {
    pub step: BatchStep,
    pub status: StepStatus,
    pub error: Option<CoreError>,
}

impl StepOutcome {
    /// A non-failing outcome (`status`, no error).
    pub fn ok(step: BatchStep, status: StepStatus) -> Self {
        Self {
            step,
            status,
            error: None,
        }
    }

    /// A failing outcome carrying the typed error.
    pub fn failed(step: BatchStep, error: CoreError) -> Self {
        Self {
            step,
            status: StepStatus::Failed,
            error: Some(error),
        }
    }
}

/// The overall batch status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    /// Every step is `Succeeded`, `Deduplicated`, or `Skipped` — none failed.
    Complete,
    /// At least one step `Failed`. Succeeded steps stay landed (no rollback).
    Partial,
}

/// The aggregate result of a batch ingest. `steps` carries one [`StepOutcome`]
/// per step in [`ALL_STEPS`] order; `guarantee` is always
/// [`TransactionGuarantee::BestEffort`] for the SQLite backend.
#[derive(Debug)]
pub struct BatchOutcome {
    pub guarantee: TransactionGuarantee,
    pub status: BatchStatus,
    pub steps: Vec<StepOutcome>,
}

impl BatchOutcome {
    /// Aggregates a per-step outcome list (in [`ALL_STEPS`] order) into a
    /// `BestEffort` outcome. The status is [`BatchStatus::Partial`] iff any
    /// step is [`StepStatus::Failed`]; otherwise [`BatchStatus::Complete`].
    /// `Skipped` and `Deduplicated` steps never make a batch `Partial`.
    pub fn from_steps(steps: Vec<StepOutcome>) -> Self {
        let status = aggregate_status(&steps);
        Self {
            guarantee: TransactionGuarantee::BestEffort,
            status,
            steps,
        }
    }
}

/// `Partial` iff any step `Failed`, else `Complete`.
pub fn aggregate_status(steps: &[StepOutcome]) -> BatchStatus {
    if steps.iter().any(|o| o.status == StepStatus::Failed) {
        BatchStatus::Partial
    } else {
        BatchStatus::Complete
    }
}

/// One semantic batch: a single `idempotency_key` + `scope`, plus each payload
/// slice (every slice is optional / empty-allowed).
#[derive(Debug, Clone)]
pub struct BatchIngestRequest {
    /// The batch idempotency key. Backends derive per-record keys from it where
    /// a store has no per-record disambiguation (e.g. the Facts step derives
    /// `{batch_key}#{index}` per memory record so N distinct records all land).
    pub idempotency_key: String,
    /// Scope isolation for every record in the batch.
    pub scope: Scope,
    /// Optional knowledge source (episode root).
    pub source: Option<KnowledgeSource>,
    /// Episode documents.
    pub documents: Vec<SourceDocument>,
    /// Episode chunks.
    pub chunks: Vec<KnowledgeChunk>,
    /// Memory facts.
    pub facts: Vec<MemoryRecord>,
    /// Graph entities.
    pub entities: Vec<KnowledgeEntity>,
    /// Graph relationships.
    pub relationships: Vec<KnowledgeRelationship>,
    /// Evidence links. Not written in v1 (reported `Skipped`) — evidence is
    /// embedded in the records' `Provenance.evidence`.
    pub evidence: Vec<EvidenceRef>,
    /// Embedding references. Not written in v1 (reported `Skipped`) — vector
    /// storage is a VectorIndex follow-up.
    pub embeddings: Vec<EmbeddingRef>,
}

/// Best-effort batch ingest port.
///
/// `transaction_guarantee()` reports the backend's guarantee
/// ([`TransactionGuarantee::BestEffort`] for SQLite). `ingest()` runs the six
/// steps in [`ALL_STEPS`] order, continuing past a step failure, and returns a
/// per-step [`BatchOutcome`].
///
/// The port is engine-neutral (ADR-0022): it names no engine type. The SQLite
/// implementation composes `Sql*` stores in the adapters layer.
#[async_trait]
pub trait BatchIngest: Send + Sync {
    /// The transactional guarantee this backend provides.
    fn transaction_guarantee(&self) -> TransactionGuarantee;

    /// Ingests `request` best-effort, returning a per-step outcome.
    async fn ingest(&self, request: BatchIngestRequest) -> CoreResult<BatchOutcome>;
}

#[cfg(test)]
mod tests {
    //! TDD for the [`BatchIngest`] port contract: an in-memory stub returns
    //! `BestEffort` and produces a `BatchOutcome` with one `StepOutcome` per
    //! step in fixed order. All-succeed → `Complete`; one failed → `Partial`;
    //! `Skipped` does not make it `Partial`.

    use super::*;
    use engram_domain::Scope;
    use std::collections::HashMap;

    /// A stub `BatchIngest` that ignores its request and produces an outcome
    /// from a per-step status override map (steps absent from the map default
    /// to `Succeeded`). It always reports `BestEffort`.
    struct StubBatchIngest {
        overrides: HashMap<BatchStep, StepStatus>,
    }

    #[async_trait]
    impl BatchIngest for StubBatchIngest {
        fn transaction_guarantee(&self) -> TransactionGuarantee {
            TransactionGuarantee::BestEffort
        }

        async fn ingest(&self, _request: BatchIngestRequest) -> CoreResult<BatchOutcome> {
            let steps = ALL_STEPS
                .iter()
                .map(|&step| {
                    let status = self
                        .overrides
                        .get(&step)
                        .cloned()
                        .unwrap_or(StepStatus::Succeeded);
                    StepOutcome::ok(step, status)
                })
                .collect();
            Ok(BatchOutcome::from_steps(steps))
        }
    }

    fn empty_request() -> BatchIngestRequest {
        BatchIngestRequest {
            idempotency_key: "stub-batch".to_string(),
            scope: Scope {
                tenant: "tenant-stub".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            source: None,
            documents: Vec::new(),
            chunks: Vec::new(),
            facts: Vec::new(),
            entities: Vec::new(),
            relationships: Vec::new(),
            evidence: Vec::new(),
            embeddings: Vec::new(),
        }
    }

    #[test]
    fn stub_reports_best_effort_guarantee() {
        let stub = StubBatchIngest {
            overrides: HashMap::new(),
        };
        assert_eq!(
            stub.transaction_guarantee(),
            TransactionGuarantee::BestEffort
        );
    }

    #[test]
    fn all_succeed_is_complete_in_fixed_order() {
        let stub = StubBatchIngest {
            overrides: HashMap::new(),
        };
        let outcome = futures_block_on(stub.ingest(empty_request()));
        let outcome = outcome.expect("stub ingest");
        assert_eq!(outcome.guarantee, TransactionGuarantee::BestEffort);
        assert_eq!(outcome.status, BatchStatus::Complete);
        // One StepOutcome per step, in the fixed order.
        assert_eq!(outcome.steps.len(), ALL_STEPS.len());
        for (i, &step) in ALL_STEPS.iter().enumerate() {
            assert_eq!(outcome.steps[i].step, step, "step {i} in fixed order");
            assert_eq!(outcome.steps[i].status, StepStatus::Succeeded);
            assert!(outcome.steps[i].error.is_none());
        }
    }

    #[test]
    fn one_failed_step_makes_partial_others_still_present() {
        let mut overrides = HashMap::new();
        overrides.insert(BatchStep::Entities, StepStatus::Failed);
        let stub = StubBatchIngest { overrides };
        let outcome = futures_block_on(stub.ingest(empty_request())).expect("stub ingest");
        assert_eq!(outcome.status, BatchStatus::Partial);
        // The failed step is flagged; the rest still landed (Succeeded).
        let entities = outcome
            .steps
            .iter()
            .find(|o| o.step == BatchStep::Entities)
            .expect("entities step present");
        assert_eq!(entities.status, StepStatus::Failed);
    }

    #[test]
    fn skipped_does_not_make_batch_partial() {
        let mut overrides = HashMap::new();
        // Evidence + Embeddings are Skipped in v1; a fully-skipped batch is
        // still Complete (Skipped never implies failure).
        overrides.insert(BatchStep::Episode, StepStatus::Skipped);
        overrides.insert(BatchStep::Evidence, StepStatus::Skipped);
        overrides.insert(BatchStep::Embeddings, StepStatus::Skipped);
        let stub = StubBatchIngest { overrides };
        let outcome = futures_block_on(stub.ingest(empty_request())).expect("stub ingest");
        assert_eq!(
            outcome.status,
            BatchStatus::Complete,
            "Skipped steps do not make a batch Partial"
        );
    }

    #[test]
    fn deduplicated_does_not_make_batch_partial() {
        let mut overrides = HashMap::new();
        overrides.insert(BatchStep::Facts, StepStatus::Deduplicated);
        let stub = StubBatchIngest { overrides };
        let outcome = futures_block_on(stub.ingest(empty_request())).expect("stub ingest");
        assert_eq!(outcome.status, BatchStatus::Complete);
    }

    #[test]
    fn aggregate_status_reflects_failure_only() {
        // Direct unit test of the aggregation invariant.
        let all_ok = ALL_STEPS
            .iter()
            .map(|&s| StepOutcome::ok(s, StepStatus::Succeeded))
            .collect::<Vec<_>>();
        assert_eq!(aggregate_status(&all_ok), BatchStatus::Complete);

        let with_skip = vec![
            StepOutcome::ok(BatchStep::Episode, StepStatus::Succeeded),
            StepOutcome::ok(BatchStep::Evidence, StepStatus::Skipped),
        ];
        assert_eq!(aggregate_status(&with_skip), BatchStatus::Complete);

        let with_fail = vec![
            StepOutcome::ok(BatchStep::Episode, StepStatus::Succeeded),
            StepOutcome::failed(
                BatchStep::Relationships,
                CoreError::Adapter {
                    adapter: "stub".to_string(),
                    message: "boom".to_string(),
                },
            ),
        ];
        assert_eq!(aggregate_status(&with_fail), BatchStatus::Partial);
    }

    /// Drives a future on the current thread without a tokio runtime.
    fn futures_block_on<F: std::future::Future>(f: F) -> F::Output {
        // The port crate depends on async-trait but not on a runtime; use a
        // minimal single-threaded poll loop so tests need no async executor.
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: std::sync::Arc<Self>) {}
        }

        let waker = Waker::from(std::sync::Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut fut = std::pin::pin!(f);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(out) => return out,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }
}
