//! Retrieval extension ports.
//!
//! These traits are the storage-neutral retrieval boundary. Candidate
//! producers implement source-specific policy and scope checks before returning
//! results; fusion and context composition then happen without knowing which
//! store, index, graph, or provider produced each candidate.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Candidate retrieval port for one source or strategy.
///
/// A `RetrievalIndex` is defined by **what it returns** — candidates of some
/// `RetrievalTargetType` (Memory, Chunk, Entity, Relationship, …) with
/// provenance and policy attached — never by **how** it retrieves. Traversal,
/// vector similarity, lexical match, or any hybrid are the adapter's private
/// choice; the trait stays mechanism-agnostic so backends are interchangeable
/// (a graph index may be backed by Neo4j traversal or pgvector semantics).
/// Final fusion and context-budget decisions are handled later in the pipeline.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait RetrievalIndex: Send + Sync {
    /// Retrieves candidates for the request without composing the final context.
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>>;
}

/// Canonical evidence for deciding whether a candidate can survive an
/// unavailable code-authority source.
///
/// This stays inside orchestration.  It is not serialized on `RetrievalResult`
/// and cannot be supplied by an arbitrary retrieval producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEvidence {
    Ordinary,
    CodeDependent,
    Unknown,
}

/// Classification plus the canonical dependency record ids that are CODE
/// (kind or durable lineage) — the recipe-side normal-path eligibility input
/// (corrective r4, canonical 6c): a recipe holding a SUCCESSFUL eligibility
/// view must still validate a derived record's code dependencies against
/// that view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceClassification {
    pub evidence: CandidateEvidence,
    pub code_dependencies: Vec<String>,
    /// True ONLY when the classifier validated the COMPLETE dependency set
    /// (corrective r5, work package A3): every reachable reference was
    /// resolved within budget by a repository-backed resolver. The trait
    /// default and every state that stops short of a validated set (budget
    /// exhaustion, an unsettleable own-lineage read, an unbound candidate)
    /// carry `false` — aggregate-only classification cannot prove an empty
    /// dependency set, so recipes must not accept on it.
    pub complete: bool,
}

impl EvidenceClassification {
    /// A fully validated ordinary chain: every reachable reference resolved
    /// through authorized ordinary records within budget.
    pub fn ordinary() -> Self {
        Self {
            evidence: CandidateEvidence::Ordinary,
            code_dependencies: Vec::new(),
            complete: true,
        }
    }

    /// A fully validated code-dependent chain. `code_dependencies` is the
    /// COMPLETE code set the recipe must validate against its view.
    pub fn code_dependent(code_dependencies: Vec<String>) -> Self {
        Self {
            evidence: CandidateEvidence::CodeDependent,
            code_dependencies,
            complete: true,
        }
    }

    /// A fully enumerated chain in which a reachable reference refused to
    /// settle (missing/foreign/unauthorized/unresolvable): a decisive,
    /// complete denial — not an unfinished walk.
    pub fn unknown() -> Self {
        Self {
            evidence: CandidateEvidence::Unknown,
            code_dependencies: Vec::new(),
            complete: true,
        }
    }

    /// General complete form produced by repository-backed walks.
    /// Callers must guarantee `code_dependencies` is nonempty whenever
    /// `evidence` is `CodeDependent`.
    pub fn complete(evidence: CandidateEvidence, code_dependencies: Vec<String>) -> Self {
        Self {
            evidence,
            code_dependencies,
            complete: true,
        }
    }

    /// The classifier did NOT validate the complete dependency set (the
    /// aggregate-only trait default, budget exhaustion, an unsettleable
    /// own-lineage read, or an unbound candidate). Never sufficient for
    /// recipe acceptance.
    pub fn incomplete(evidence: CandidateEvidence) -> Self {
        Self {
            evidence,
            code_dependencies: Vec::new(),
            complete: false,
        }
    }
}

/// Resolves candidate evidence from the owning canonical stores under the
/// request scope and policy boundary.
#[async_trait]
pub trait CandidateEvidenceResolver: Send + Sync {
    async fn evidence(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<CandidateEvidence>;

    /// Full classification with code dependency ids. The default resolves
    /// only the aggregate evidence (implementors backed by the owning
    /// canonical stores provide the dependency detail).
    async fn classify_with_dependencies(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<EvidenceClassification> {
        // Aggregate-only classification cannot prove an empty dependency
        // set (work package A3): the default is explicitly incomplete.
        Ok(EvidenceClassification::incomplete(
            self.evidence(candidate, request).await?,
        ))
    }
}

/// Merges and reranks candidates from multiple retrieval sources.
///
/// Implementations should preserve or populate `FusionTrace` so callers can see
/// which source contributed a result, how scores changed, and which duplicates
/// were collapsed.
pub trait RetrievalFusion: Send + Sync {
    /// Returns a ranked candidate list after fusion and optional reranking.
    fn fuse(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>>;
}

/// Builds the final context payload returned to callers.
///
/// Composition is where budgets, omitted-result explanations, and non-fatal
/// source failures become visible. Implementations must not hide policy denials
/// or degraded retrieval sources when the contract allows reporting them.
pub trait ContextComposer: Send + Sync {
    /// Applies final budget and explanation rules to produce caller context.
    fn compose(
        &self,
        request: &RetrievalRequest,
        results: Vec<RetrievalResult>,
        failures: Vec<RetrievalSourceFailure>,
    ) -> CoreResult<ContextPayload>;
}

/// Reranks fused retrieval candidates by query-aware relevance (cross-encoder).
/// Applied between fusion and budget in `compose_context`. Implementations stay
/// behind the injected scorer; model lifecycle is never in core.
pub trait RetrievalReranker: Send + Sync {
    /// Reranks candidates by the query, best-first. Stamps `FusionTrace` with
    /// `rerank_strategy = CrossEncoder` + the new score. Returns the reranked
    /// list (may be shorter if candidates are filtered).
    fn rerank(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>>;
}
