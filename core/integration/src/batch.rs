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
//! A **sourced batch** (one carrying [`BatchIngestRequest::source_id`]) adds
//! durable source provenance and revision supersession on top of the
//! best-effort model: every written assertion is stamped with a source
//! `EvidenceRef`, a per-source ledger binds each batch to the record ids it
//! wrote, and a later batch for the same `(scope, source_id)` supersedes the
//! source's prior assertions (archive for graph records, tombstone forget for
//! facts, ref-counted shared records). Per-step supersession counts surface on
//! [`StepOutcome::supersession`]. An **unsourced** batch is byte-identical to
//! the legacy append-only behavior: no stamping, no ledger rows, no
//! supersession (`supersession` stays `None`).
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
use serde::{Deserialize, Serialize};

/// The transactional guarantee a [`BatchIngest`] backend provides.
///
/// The SQLite backend is [`BestEffort`](Self::BestEffort): separate store files
/// cannot share a transaction, so a step failure does not roll back earlier
/// steps. `Atomic` is reserved for a future single-connection backend that can
/// offer true cross-store ACID; no v1 backend returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
///
/// `supersession` carries the per-step supersession counts and is `Some` only
/// for sourced batches (unsourced steps stay `None`, keeping legacy outcomes
/// byte-identical).
#[derive(Debug)]
pub struct StepOutcome {
    pub step: BatchStep,
    pub status: StepStatus,
    pub error: Option<CoreError>,
    /// Per-step supersession counts (sourced batches only; `None` unsourced).
    pub supersession: Option<SupersessionCounts>,
}

impl StepOutcome {
    /// A non-failing outcome (`status`, no error, no supersession counts).
    pub fn ok(step: BatchStep, status: StepStatus) -> Self {
        Self {
            step,
            status,
            error: None,
            supersession: None,
        }
    }

    /// A failing outcome carrying the typed error (no supersession counts).
    pub fn failed(step: BatchStep, error: CoreError) -> Self {
        Self {
            step,
            status: StepStatus::Failed,
            error: Some(error),
            supersession: None,
        }
    }

    /// Builder: attaches supersession counts to this outcome (sourced steps).
    /// `StepOutcome::ok(step, status).with_counts(counts)` — the `ok`/`failed`
    /// constructors stay source-compatible.
    pub fn with_counts(mut self, counts: SupersessionCounts) -> Self {
        self.supersession = Some(counts);
        self
    }
}

/// Per-step supersession counts for a sourced batch (surfaced, never hidden —
/// a partial supersation is visible as landed counts plus a `failed` count).
/// Every counter is zero unless this step superseded prior assertions of the
/// same source. Unsourced batches never construct this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SupersessionCounts {
    /// Prior ledger rows this step examined for the source.
    pub examined: usize,
    /// Prior facts forgotten (tombstone mode).
    pub forgotten_facts: usize,
    /// Prior entities archived (last source ref removed).
    pub archived_entities: usize,
    /// Prior relationships archived (last source ref removed).
    pub archived_relationships: usize,
    /// Source refs removed from records that stay active (shared records).
    pub refs_removed: usize,
    /// Re-asserted records restored from a prior archive.
    pub restored: usize,
    /// Prior rows skipped because the record no longer carries this source's
    /// ref (immune — e.g. code-scan-derived records, never matched by name).
    pub immune_skipped: usize,
    /// Per-item supersession failures (best-effort: surfaced, never rolled back).
    pub failed: usize,
}

/// The overall batch status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Optional caller-declared, revision-stable source identity for a
    /// **sourced batch** (e.g. `"docs/x.md"`). Distinct from
    /// `idempotency_key` (retry-scoped): the source identity survives
    /// revisions. When present, the backend stamps every written assertion
    /// with a source `EvidenceRef`, records written record ids in a per-source
    /// ledger, and supersedes the prior batches of the same
    /// `(scope, source_id)` (archive for entities/relationships, tombstone
    /// forget for facts, ref-counted shared records — per-step counts on
    /// [`StepOutcome::supersession`]). When absent, behavior is byte-identical
    /// to the legacy append-only path. Additive: missing in old payloads ⇒
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// Best-effort batch ingest port.
///
/// `transaction_guarantee()` reports the backend's guarantee
/// ([`TransactionGuarantee::BestEffort`] for SQLite). `ingest()` runs the six
/// steps in [`ALL_STEPS`] order, continuing past a step failure, and returns a
/// per-step [`BatchOutcome`].
///
/// **Sourced batches** (a [`BatchIngestRequest::source_id`]) additionally
/// stamp source provenance on every written assertion, ledger the written
/// record ids, and supersede the same source's prior batches
/// (declared-set reconciliation by ledger id + source-ref membership — never
/// by idempotency-key equality or entity name). Supersession items soft-fail
/// per record inside their step; counts surface on
/// [`StepOutcome::supersession`] — a partial supersession is visible, never
/// rolled back.
///
/// The port is engine-neutral (ADR-0022): it names no engine type. The SQLite
/// implementation composes `Sql*` stores in the adapters layer.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
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
            source_id: None,
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

    #[test]
    fn constructors_leave_supersession_none_and_with_counts_attaches() {
        // AC-006 shape guard: the legacy constructors produce outcomes with no
        // supersession counts, so unsourced batches stay byte-identical; the
        // `with_counts` builder is the additive sourced path.
        let ok = StepOutcome::ok(BatchStep::Facts, StepStatus::Succeeded);
        assert!(ok.supersession.is_none());
        let failed = StepOutcome::failed(
            BatchStep::Facts,
            CoreError::Adapter {
                adapter: "stub".to_string(),
                message: "boom".to_string(),
            },
        );
        assert!(failed.supersession.is_none());

        let counts = SupersessionCounts {
            examined: 3,
            forgotten_facts: 2,
            archived_entities: 1,
            ..SupersessionCounts::default()
        };
        let sourced = StepOutcome::ok(BatchStep::Facts, StepStatus::Succeeded).with_counts(counts);
        assert_eq!(sourced.supersession, Some(counts));
        // The builder only attaches counts — status/error are untouched.
        assert_eq!(sourced.status, StepStatus::Succeeded);
        assert!(sourced.error.is_none());
        // Default is all-zero (a first ingest / idempotent second pass reports
        // zeros, never fabricated counts).
        assert_eq!(
            SupersessionCounts::default(),
            SupersessionCounts {
                examined: 0,
                forgotten_facts: 0,
                archived_entities: 0,
                archived_relationships: 0,
                refs_removed: 0,
                restored: 0,
                immune_skipped: 0,
                failed: 0,
            }
        );
    }

    #[test]
    fn supersession_counts_serde_is_camel_case() {
        let counts = SupersessionCounts {
            examined: 4,
            forgotten_facts: 2,
            archived_entities: 1,
            archived_relationships: 0,
            refs_removed: 1,
            restored: 0,
            immune_skipped: 0,
            failed: 0,
        };
        let json = serde_json::to_string(&counts).expect("serialize counts");
        assert!(json.contains("\"forgottenFacts\":2"), "camelCase: {json}");
        assert!(json.contains("\"archivedEntities\":1"), "camelCase: {json}");
        let back: SupersessionCounts = serde_json::from_str(&json).expect("deserialize counts");
        assert_eq!(back, counts);
    }

    #[test]
    fn source_id_is_additive_legacy_payloads_decode() {
        // NFR-004: a legacy payload (serialized before source_id existed)
        // decodes with source_id = None, and a sourced payload round-trips.
        // (BatchIngestRequest fields serialize snake_case — no rename_all. The
        // other fields stay required in JSON exactly as before; only
        // source_id is additive-optional.)
        let legacy = serde_json::to_value(empty_request()).expect("serialize");
        assert!(legacy.get("source_id").is_none(), "None is omitted");
        let decoded: BatchIngestRequest =
            serde_json::from_value(legacy).expect("legacy payload decodes");
        assert_eq!(decoded.source_id, None);

        let mut sourced = empty_request();
        sourced.source_id = Some("docs/x.md".to_string());
        let round: BatchIngestRequest =
            serde_json::from_value(serde_json::to_value(&sourced).expect("serialize"))
                .expect("round trip");
        assert_eq!(round.source_id.as_deref(), Some("docs/x.md"));
    }

    /// Drives a future on the current thread without a tokio runtime.
    fn futures_block_on<F: std::future::Future>(f: F) -> F::Output {
        // The port crate depends on async-trait but not on a runtime; use a
        // minimal single-threaded poll loop so tests need no async executor.
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
