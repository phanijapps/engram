//! Canonical candidate evidence used only while code eligibility is unavailable.
//!
//! Every classification resolves the candidate's canonical record through the
//! OWNING repositories under the request scope, authorizes the read through
//! the same configured [`PolicyAuthorizer`] the owning retrieval path uses
//! (`request.requester` + the record's canonical scope/policy — policy
//! equality alone is never authorization), binds the candidate payload to the
//! canonical bytes/revision through the lanes' DETERMINISTIC PROJECTIONS,
//! and then walks the record's canonical dependency chain — its own
//! provenance AND its typed origins (chunk source provenance, entity
//! `source_refs`, belief `sources` + synthesizer inputs) — with CYCLE,
//! DEPTH, RECORD, REFERENCE and BYTE bounds that are all checked BEFORE any
//! cloning, insertion or enqueueing.
//!
//! Work package A3 (corrective r5) makes the walk's state explicit and
//! fail-closed:
//! - OWN IDENTITY is classified for EVERY candidate kind (Memory and Belief
//!   included, and references to them): a record is code by kind or by its
//!   BOUNDED KEYED durable-lineage read. An unconfigured ownership reader
//!   cannot prove the negative and leaves the whole classification
//!   incomplete.
//! - CODE-OWNED records CONTINUE their own dependency resolution — a code
//!   finding never terminates the walk, so a stale, denied or missing
//!   sibling cannot hide behind an eligible code member (Unknown dominates
//!   the aggregate and siblings continue after a code hit).
//! - Budget exhaustion is a TERMINAL INCOMPLETE Unknown, distinct from a
//!   fully evaluated ordinary or unresolved result.
//! - Only a walk that validated the COMPLETE dependency set within budget
//!   produces a `complete` classification; the trait default is explicitly
//!   incomplete (aggregate-only classification cannot prove an empty
//!   dependency set).
//!
//! Mismatched payloads, foreign/missing records, unauthorized reads, ID-less
//! derived references, canonically unresolvable reference categories,
//! ambiguous typed origins, cycles that cannot be settled and work/depth
//! overruns all degrade to `Unknown` — ambiguity never becomes ordinary
//! evidence (corrective r4, canonical 6a/6b/6d).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use engram_belief::BeliefRepository;
use engram_domain::{
    BeliefSource, EvidenceRef, EvidenceTargetType, KnowledgeChunkKind, Policy, RetrievalRequest,
    RetrievalResult, RetrievalTargetType,
};
use engram_knowledge::{CodeBindingRepository, KnowledgeGraphRepository, KnowledgeRepository};
use engram_memory::MemoryRepository;
use engram_retrieval::{CandidateEvidence, CandidateEvidenceResolver, EvidenceClassification};
use engram_runtime::{CoreResult, PolicyAuthorizer};

/// Maximum dependency-chain depth evaluated per candidate. Deeper ambiguity
/// degrades to `Unknown` rather than an unbounded walk.
const MAX_DEPENDENCY_DEPTH: usize = 8;
/// Maximum canonical records resolved per candidate evaluation.
const MAX_DEPENDENCY_RECORDS: usize = 256;
/// TOTAL reference examinations (work) per candidate evaluation — enforced
/// BEFORE any per-reference cloning or enqueueing so neither processing nor
/// allocation grows with a hostile dependency array (canonical 6d).
const MAX_DEPENDENCY_REFERENCES: usize = 1024;
/// TOTAL bytes of reference identifiers the walk may copy into its own
/// state (visited keys, code-dependency ids, queued frames) — checked
/// BEFORE every clone, insertion or enqueue (work package A3).
const MAX_DEPENDENCY_BYTES: usize = 128 * 1024;

/// Instrumented walk counters (work package A3 + review r5 finding 5):
/// reference examinations, ADMISSION-scan work (frame construction scans
/// — bounded by the remaining budgets, never a full hostile-array preflight),
/// keyed ownership lookups, frame enqueues, queue/visited maxima, byte
/// accounting (ALL charges: yielded references, visited keys and code-
/// dependency ids), terminal exhaustions and cycle back-edges. Bounds are
/// proven against these counters, never against elapsed time.
#[derive(Default, Debug)]
pub struct WalkCounters {
    references_examined: AtomicUsize,
    admission_examined: AtomicUsize,
    synthetic_refs: AtomicUsize,
    own_id_charges: AtomicUsize,
    ownership_lookups: AtomicUsize,
    frames_enqueued: AtomicUsize,
    max_queue_depth: AtomicUsize,
    visited_records: AtomicUsize,
    bytes_accounted: AtomicUsize,
    budget_exhaustions: AtomicUsize,
    cycle_backedges: AtomicUsize,
}

/// Point-in-time snapshot of [`WalkCounters`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkStats {
    pub references_examined: usize,
    /// Admission examinations CHARGED against the SHARED admission-work
    /// budget (finding 5): every evidence ref, derivation entry, typed ref
    /// and belief source a frame constructor examines consumes the ONE
    /// persistent, monotonically consumed work budget BEFORE it is
    /// admitted — so this counter is bounded by that budget, never by the
    /// input's width. Structural charges (derivation flattening, frame-pop
    /// probes) consume the same budget without appearing here.
    pub admission_examined: usize,
    /// SYNTHETIC dependency-reference constructions (chunk source
    /// dependencies, relationship endpoints) budget-checked BEFORE their
    /// id String is allocated (finding 5).
    pub synthetic_refs: usize,
    /// Candidate OWN dependency-id charge attempts (finding 5): the
    /// `own_id.to_owned()` allocation in `merge_own` is byte-budgeted
    /// BEFORE allocation; the counter instruments each charge attempt.
    pub own_id_charges: usize,
    pub ownership_lookups: usize,
    pub frames_enqueued: usize,
    pub max_queue_depth: usize,
    pub visited_records: usize,
    pub bytes_accounted: usize,
    pub budget_exhaustions: usize,
    pub cycle_backedges: usize,
}

impl WalkCounters {
    /// Reads every counter once (relaxed loads; the counters are
    /// instrumentation, not synchronization).
    pub fn snapshot(&self) -> WalkStats {
        WalkStats {
            references_examined: self.references_examined.load(Ordering::Relaxed),
            admission_examined: self.admission_examined.load(Ordering::Relaxed),
            synthetic_refs: self.synthetic_refs.load(Ordering::Relaxed),
            own_id_charges: self.own_id_charges.load(Ordering::Relaxed),
            ownership_lookups: self.ownership_lookups.load(Ordering::Relaxed),
            frames_enqueued: self.frames_enqueued.load(Ordering::Relaxed),
            max_queue_depth: self.max_queue_depth.load(Ordering::Relaxed),
            visited_records: self.visited_records.load(Ordering::Relaxed),
            bytes_accounted: self.bytes_accounted.load(Ordering::Relaxed),
            budget_exhaustions: self.budget_exhaustions.load(Ordering::Relaxed),
            cycle_backedges: self.cycle_backedges.load(Ordering::Relaxed),
        }
    }

    /// Resets every counter to zero.
    pub fn reset(&self) {
        for counter in [
            &self.references_examined,
            &self.admission_examined,
            &self.synthetic_refs,
            &self.own_id_charges,
            &self.ownership_lookups,
            &self.frames_enqueued,
            &self.max_queue_depth,
            &self.visited_records,
            &self.bytes_accounted,
            &self.budget_exhaustions,
            &self.cycle_backedges,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

/// One record's OWN-IDENTITY classification: its kind and durable lineage
/// decide code ownership independently of the dependency walk (work package
/// A3 — own identity, complete dependency set, unresolved evidence and
/// budget exhaustion are tracked SEPARATELY).
enum OwnIdentity {
    /// The record is code by kind or durable lineage; its id joins the
    /// code-dependency set for the recipe's view validation.
    Code,
    /// Positively ordinary: non-code kind and NO durable lineage.
    Ordinary,
    /// The ownership reader is unconfigured: own lineage cannot be settled,
    /// so no positive classification is possible.
    Unsettled,
}

/// One walked member's contribution to the aggregate classification — the
/// explicit unresolved/unsettled/exhausted states of work package A3.
enum MemberEvidence {
    /// Cycle back-edge to a record already visited in this walk: adds no
    /// new evidence (the monoid identity); the cycle's true classification
    /// is settled by the rest of the walk.
    Identity,
    /// A positively authorized ordinary record.
    Ordinary,
    /// A code record (kind or durable lineage): its id joined the
    /// code-dependency set and its own dependencies CONTINUE to walk.
    Code,
    /// Decisive unresolved evidence (missing, foreign, unauthorized,
    /// ID-less or canonically unresolvable reference): complete denial.
    Unresolved,
    /// Own lineage could not be settled (ownership reader unconfigured):
    /// the final classification is incomplete regardless of the aggregate.
    Unsettled,
    /// TERMINAL budget/depth exhaustion: the dependency set was not
    /// validated; the whole walk stops.
    Exhausted,
}

/// One queued unit of dependency work. Records are MOVED out of fetched
/// results into frames (the walk never re-clones whole provenance/source
/// arrays); cursors iterate the moved arrays in place. `remaining` and
/// `remaining_bytes` admit the frame against the work/byte budgets BEFORE
/// it is enqueued.
enum Frame {
    /// A provenance frame: `evidence` refs first, then each derivation's
    /// `input_refs` (flattened by move once the evidence is exhausted).
    Provenance {
        provenance: engram_domain::Provenance,
        index: usize,
        depth: usize,
        remaining: usize,
        remaining_bytes: usize,
    },
    /// A moved list of typed evidence refs (entity `source_refs`,
    /// relationship evidence, synthesizer `input_refs`).
    Refs {
        refs: Vec<EvidenceRef>,
        index: usize,
        depth: usize,
        remaining: usize,
        remaining_bytes: usize,
    },
    /// A belief's typed `sources` (moved from the record; unresolvable
    /// target kinds contribute `Unresolvable` directly, with no synthetic
    /// reference allocation).
    BeliefSources {
        sources: Vec<BeliefSource>,
        index: usize,
        depth: usize,
        remaining: usize,
        remaining_bytes: usize,
    },
}

/// The next reference a frame yields, borrowed in place; the caller (or
/// [`admit_reference`]) clones the id only after the byte budget admits
/// it.
enum FrameRef<'a> {
    Typed(EvidenceTargetType, Option<&'a str>),
    Unresolvable,
}

/// The outcome of advancing one frame: an admitted reference (byte-budget
/// checked BEFORE the bounded clone), an unresolvable reference (no clone),
/// a byte-budget rejection (terminal), or frame exhaustion.
enum Advance {
    Reference(EvidenceTargetType, String),
    Unresolvable,
    Rejected,
    Done,
}

/// Admits one yielded reference against the byte budget BEFORE cloning its
/// id; `Rejected` is a terminal budget exhaustion.
fn admit_reference(frame_ref: FrameRef<'_>, state: &mut WalkState) -> Advance {
    match frame_ref {
        FrameRef::Unresolvable | FrameRef::Typed(_, None) => Advance::Unresolvable,
        FrameRef::Typed(target_type, Some(id)) => {
            if state.bytes + id.len() + 1 > MAX_DEPENDENCY_BYTES {
                return Advance::Rejected;
            }
            state.bytes += id.len() + 1;
            Advance::Reference(target_type, id.to_owned())
        }
    }
}

fn ref_bytes(reference: &EvidenceRef) -> usize {
    reference.target_id.as_ref().map_or(1, |id| id.len() + 1)
}

fn belief_source_bytes(source: &BeliefSource) -> usize {
    source.target_id.len() + 1
}

/// Maps one canonical evidence ref to its frame reference; categories with
/// no canonical resolution are unresolvable (canonical 6a).
fn frame_ref_of(reference: &EvidenceRef) -> FrameRef<'_> {
    match reference.target_type {
        EvidenceTargetType::Document
        | EvidenceTargetType::Concept
        | EvidenceTargetType::Event
        | EvidenceTargetType::Url => FrameRef::Unresolvable,
        ref target_type => FrameRef::Typed(target_type.clone(), reference.target_id.as_deref()),
    }
}

/// Maps one typed belief source to its frame reference; assertion/event/
/// concept targets have no canonical EvidenceRef equivalent and are
/// unresolvable without any synthetic reference allocation.
fn belief_source_frame_ref(source: &BeliefSource) -> FrameRef<'_> {
    match source.target_type {
        engram_domain::BeliefSourceTargetType::Memory => {
            FrameRef::Typed(EvidenceTargetType::Memory, Some(source.target_id.as_str()))
        }
        engram_domain::BeliefSourceTargetType::Chunk => {
            FrameRef::Typed(EvidenceTargetType::Chunk, Some(source.target_id.as_str()))
        }
        engram_domain::BeliefSourceTargetType::Entity => {
            FrameRef::Typed(EvidenceTargetType::Entity, Some(source.target_id.as_str()))
        }
        engram_domain::BeliefSourceTargetType::Relationship => FrameRef::Typed(
            EvidenceTargetType::Relationship,
            Some(source.target_id.as_str()),
        ),
        engram_domain::BeliefSourceTargetType::Belief => {
            FrameRef::Typed(EvidenceTargetType::Belief, Some(source.target_id.as_str()))
        }
        engram_domain::BeliefSourceTargetType::Assertion
        | engram_domain::BeliefSourceTargetType::Event
        | engram_domain::BeliefSourceTargetType::Concept => FrameRef::Unresolvable,
    }
}
impl Frame {
    /// Byte-budget fit for the admission scan's cumulative reservation
    /// against the PERSISTENT byte budget (finding 5).
    fn fits_bytes(state: &WalkState, bytes: usize) -> bool {
        state.bytes + bytes <= MAX_DEPENDENCY_BYTES
    }

    /// Builds a provenance frame from MOVED provenance with an
    /// EARLY-TERMINATING admission scan charged against the ONE SHARED,
    /// monotonically consumed admission-work budget (review r5, finding 5
    /// — verification finding 3): evidence references, then each
    /// derivation — whose STRUCTURAL entry is itself admission work (an
    /// array of empty derivations cannot force an unbounded preflight) —
    /// then its input refs. Every examined item charges the PERSISTENT
    /// `state.work` BEFORE it is admitted, so distributed derivation-heavy
    /// records exhaust the TOTAL budget instead of reserving
    /// constructor-local room that is never paid. `None` means the shared
    /// budget or the byte reservation cannot fit this item (terminal
    /// exhaustion for the caller; the charges already made are consumed,
    /// never refunded).
    fn provenance(
        provenance: engram_domain::Provenance,
        depth: usize,
        state: &mut WalkState,
        counters: &WalkCounters,
    ) -> Option<Self> {
        let mut remaining = 0usize;
        let mut remaining_bytes = 0usize;
        for reference in &provenance.evidence {
            if !state.charge_admission(counters) {
                return None;
            }
            remaining += 1;
            remaining_bytes += ref_bytes(reference);
            if !Self::fits_bytes(state, remaining_bytes) {
                return None;
            }
        }
        for derivation in &provenance.derivations {
            if !state.charge_admission(counters) {
                return None;
            }
            remaining += 1;
            if !Self::fits_bytes(state, remaining_bytes) {
                return None;
            }
            for reference in &derivation.input_refs {
                if !state.charge_admission(counters) {
                    return None;
                }
                remaining += 1;
                remaining_bytes += ref_bytes(reference);
                if !Self::fits_bytes(state, remaining_bytes) {
                    return None;
                }
            }
        }
        Some(Self::Provenance {
            provenance,
            index: 0,
            depth,
            remaining,
            remaining_bytes,
        })
    }

    /// Builds a typed-refs frame from MOVED refs with the same
    /// shared-budget-charged early-terminating admission scan.
    fn refs(
        refs: Vec<EvidenceRef>,
        depth: usize,
        state: &mut WalkState,
        counters: &WalkCounters,
    ) -> Option<Self> {
        let mut remaining = 0usize;
        let mut remaining_bytes = 0usize;
        for reference in &refs {
            if !state.charge_admission(counters) {
                return None;
            }
            remaining += 1;
            remaining_bytes += ref_bytes(reference);
            if !Self::fits_bytes(state, remaining_bytes) {
                return None;
            }
        }
        Some(Self::Refs {
            refs,
            index: 0,
            depth,
            remaining,
            remaining_bytes,
        })
    }

    /// Builds a typed belief-sources frame from MOVED sources with the
    /// same shared-budget-charged early-terminating admission scan.
    fn belief_sources(
        sources: Vec<BeliefSource>,
        depth: usize,
        state: &mut WalkState,
        counters: &WalkCounters,
    ) -> Option<Self> {
        let mut remaining = 0usize;
        let mut remaining_bytes = 0usize;
        for source in &sources {
            if !state.charge_admission(counters) {
                return None;
            }
            remaining += 1;
            remaining_bytes += belief_source_bytes(source);
            if !Self::fits_bytes(state, remaining_bytes) {
                return None;
            }
        }
        Some(Self::BeliefSources {
            sources,
            index: 0,
            depth,
            remaining,
            remaining_bytes,
        })
    }

    fn depth(&self) -> usize {
        match self {
            Frame::Provenance { depth, .. }
            | Frame::Refs { depth, .. }
            | Frame::BeliefSources { depth, .. } => *depth,
        }
    }

    fn remaining_bytes(&self) -> usize {
        match self {
            Frame::Provenance {
                remaining_bytes, ..
            }
            | Frame::Refs {
                remaining_bytes, ..
            }
            | Frame::BeliefSources {
                remaining_bytes, ..
            } => *remaining_bytes,
        }
    }

    /// Yields the next reference with the byte budget checked BEFORE the
    /// bounded id clone, accounting the reference against the frame's
    /// precomputed footprint. When a provenance frame's evidence is
    /// exhausted its derivations are flattened ONCE by move into a refs
    /// frame (no clone), with the structural flatten work charged to the
    /// SAME shared admission budget. `Advance::Rejected` covers BOTH the
    /// yield-time byte rejection and a budget-exhausted flatten (terminal
    /// exhaustion in either case).
    fn advance(&mut self, state: &mut WalkState, counters: &WalkCounters) -> Advance {
        // Provenance frames yield evidence refs until exhausted.
        let provenance_exhausted = match self {
            Frame::Provenance {
                provenance,
                index,
                remaining,
                remaining_bytes,
                ..
            } => match provenance.evidence.get(*index) {
                Some(reference) => {
                    *index += 1;
                    *remaining = remaining.saturating_sub(1);
                    *remaining_bytes = remaining_bytes.saturating_sub(ref_bytes(reference));
                    return admit_reference(frame_ref_of(reference), state);
                }
                None => true,
            },
            _ => false,
        };
        if provenance_exhausted && !self.flatten_derivations(state, counters) {
            return Advance::Rejected;
        }
        self.advance_typed(state)
    }

    /// Flattens a provenance frame's derivations into a moved refs frame
    /// (once; the placeholder is overwritten immediately). Flattening is
    /// STRUCTURAL work charged to the ONE shared admission-work budget
    /// (review r5, finding 5 — verification finding 3): each derivation
    /// the flatten moves charges the budget, so admitted-and-flattened
    /// derivation structures cannot be processed outside byte/work
    /// accounting. `false` means the shared budget is exhausted (terminal).
    fn flatten_derivations(&mut self, state: &mut WalkState, counters: &WalkCounters) -> bool {
        let (refs, depth, remaining, remaining_bytes) = match std::mem::replace(
            self,
            Frame::Refs {
                refs: Vec::new(),
                index: 0,
                depth: 0,
                remaining: 0,
                remaining_bytes: 0,
            },
        ) {
            Frame::Provenance {
                provenance,
                depth,
                remaining,
                remaining_bytes,
                ..
            } => {
                let derivations = provenance.derivations;
                for _ in &derivations {
                    if !state.charge_admission(counters) {
                        return false;
                    }
                }
                (
                    derivations
                        .into_iter()
                        .flat_map(|derivation| derivation.input_refs)
                        .collect::<Vec<_>>(),
                    depth,
                    remaining,
                    remaining_bytes,
                )
            }
            other => {
                *self = other;
                return true;
            }
        };
        *self = Frame::Refs {
            refs,
            index: 0,
            depth,
            remaining,
            remaining_bytes,
        };
        true
    }

    /// Advances a refs/belief-sources frame (provenance frames never reach
    /// this helper after their derivations were flattened).
    fn advance_typed(&mut self, state: &mut WalkState) -> Advance {
        match self {
            Frame::Refs {
                refs,
                index,
                remaining,
                remaining_bytes,
                ..
            } => match refs.get(*index) {
                Some(reference) => {
                    *index += 1;
                    *remaining = remaining.saturating_sub(1);
                    *remaining_bytes = remaining_bytes.saturating_sub(ref_bytes(reference));
                    admit_reference(frame_ref_of(reference), state)
                }
                None => Advance::Done,
            },
            Frame::BeliefSources {
                sources,
                index,
                remaining,
                remaining_bytes,
                ..
            } => match sources.get(*index) {
                Some(source) => {
                    *index += 1;
                    *remaining = remaining.saturating_sub(1);
                    *remaining_bytes = remaining_bytes.saturating_sub(belief_source_bytes(source));
                    admit_reference(belief_source_frame_ref(source), state)
                }
                None => Advance::Done,
            },
            Frame::Provenance { .. } => Advance::Done,
        }
    }
}

/// Mutable walk state: visited keys, the work/byte budgets, the aggregate
/// classification, the unsettled flag and the code-dependency set.
#[derive(Default)]
struct WalkState {
    resolved: BTreeSet<(u8, String)>,
    work: usize,
    bytes: usize,
    aggregate: Aggregate,
    unsettled: bool,
    code_dependencies: Vec<String>,
}

/// Aggregate classification with `Ordinary` as the identity — kept separate
/// from [`MemberEvidence`] so no member state can be conflated with the
/// fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Aggregate {
    #[default]
    Identity,
    Code,
    Unknown,
}

impl Aggregate {
    fn fold(self, next: Aggregate) -> Aggregate {
        match (self, next) {
            (Aggregate::Unknown, _) | (_, Aggregate::Unknown) => Aggregate::Unknown,
            (Aggregate::Code, _) | (_, Aggregate::Code) => Aggregate::Code,
            (Aggregate::Identity, Aggregate::Identity) => Aggregate::Identity,
        }
    }

    fn evidence(self) -> CandidateEvidence {
        match self {
            Aggregate::Identity => CandidateEvidence::Ordinary,
            Aggregate::Code => CandidateEvidence::CodeDependent,
            Aggregate::Unknown => CandidateEvidence::Unknown,
        }
    }
}

impl WalkState {
    /// Charges ONE admission examination against the ONE shared,
    /// monotonically consumed admission-work budget (review r5, finding 5
    /// — verification finding 3). Check-first: `work` NEVER exceeds
    /// [`MAX_DEPENDENCY_REFERENCES`]; `false` means the shared budget is
    /// exhausted (terminal for the caller). A charge already made is never
    /// refunded — a later byte-fit rejection still consumed the
    /// examination work.
    fn charge_admission(&mut self, counters: &WalkCounters) -> bool {
        if self.work >= MAX_DEPENDENCY_REFERENCES {
            return false;
        }
        self.work += 1;
        counters.admission_examined.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Charges the candidate's OWN dependency-id bytes BEFORE the
    /// allocation that consumes them (`merge_own`'s `own_id.to_owned()`,
    /// finding 5 — verification finding 3): the reservation is charged
    /// up-front and never refunded; an oversized own id is terminal
    /// admission exhaustion, never an unmetered allocation. Each charge
    /// ATTEMPT is instrumented, including the rejected one.
    fn charge_own_id(&mut self, counters: &WalkCounters, own_id: &str) -> bool {
        counters.own_id_charges.fetch_add(1, Ordering::Relaxed);
        if self.bytes + own_id.len() + 1 > MAX_DEPENDENCY_BYTES {
            return false;
        }
        self.bytes += own_id.len() + 1;
        true
    }

    /// Budgets a SYNTHETIC dependency-reference id allocation BEFORE the
    /// String is built (finding 5 — verification finding 3):
    /// `source_dependency_ref` and `endpoint_ref` allocate owned ids from
    /// borrowed records; the byte budget gates that allocation. The
    /// admitted reference's bytes are charged at YIELD time like any other
    /// yielded reference — this is the pre-allocation gate, not a second
    /// charge. Each budgeted construction is instrumented.
    fn admits_synthetic_ref(&self, counters: &WalkCounters, id_len: usize) -> bool {
        counters.synthetic_refs.fetch_add(1, Ordering::Relaxed);
        self.bytes + id_len + 1 <= MAX_DEPENDENCY_BYTES
    }

    /// Admits one code-dependency id against the byte budget BEFORE the
    /// bounded clone; `false` means terminal exhaustion.
    fn admit_code_dependency(&mut self, id: &str) -> bool {
        if self.bytes + id.len() + 1 > MAX_DEPENDENCY_BYTES {
            return false;
        }
        self.bytes += id.len() + 1;
        self.code_dependencies.push(id.to_owned());
        true
    }

    /// Admits one visited key against the byte budget BEFORE the bounded
    /// clone; `false` means terminal exhaustion.
    fn admit_visited(&mut self, key: (u8, String)) -> bool {
        if self.bytes + key.1.len() + 1 > MAX_DEPENDENCY_BYTES {
            return false;
        }
        self.bytes += key.1.len() + 1;
        self.resolved.insert(key);
        true
    }

    /// Terminal classification for budget exhaustion — the dependency set
    /// was NOT validated, so the result is explicitly incomplete (distinct
    /// from a fully evaluated ordinary or unresolved result).
    fn exhausted(&self) -> EvidenceClassification {
        EvidenceClassification::incomplete(CandidateEvidence::Unknown)
    }

    /// Final classification: only a walk that finished without exhaustion
    /// or an unsettled own-identity read is complete.
    fn finish(self) -> EvidenceClassification {
        if self.unsettled {
            return EvidenceClassification::incomplete(CandidateEvidence::Unknown);
        }
        match self.aggregate.evidence() {
            CandidateEvidence::Ordinary => EvidenceClassification::ordinary(),
            CandidateEvidence::CodeDependent => {
                EvidenceClassification::code_dependent(self.code_dependencies)
            }
            CandidateEvidence::Unknown => EvidenceClassification::unknown(),
        }
    }
}

/// Compact resolution key for cycle/work bounding. `EvidenceTargetType` is a
/// wire enum without `Ord`/`Hash`, so the walk keys on a stable per-variant
/// discriminant plus the target id.
fn target_discriminant(target_type: &EvidenceTargetType) -> u8 {
    match target_type {
        EvidenceTargetType::Memory => 1,
        EvidenceTargetType::Event => 2,
        EvidenceTargetType::Source => 3,
        EvidenceTargetType::Document => 4,
        EvidenceTargetType::Chunk => 5,
        EvidenceTargetType::Entity => 6,
        EvidenceTargetType::Concept => 7,
        EvidenceTargetType::Url => 8,
        EvidenceTargetType::Relationship => 9,
        EvidenceTargetType::Belief => 10,
    }
}

/// Reads each capability's canonical scoped record before permitting degraded recall.
pub struct RepositoryCandidateEvidence {
    memory: Arc<dyn MemoryRepository>,
    beliefs: Arc<dyn BeliefRepository>,
    knowledge: Arc<dyn KnowledgeRepository>,
    authorizer: Arc<dyn PolicyAuthorizer>,
    ownership: Option<Arc<dyn CodeBindingRepository>>,
    graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    counters: Arc<WalkCounters>,
}

impl RepositoryCandidateEvidence {
    pub fn new(
        memory: Arc<dyn MemoryRepository>,
        beliefs: Arc<dyn BeliefRepository>,
        knowledge: Arc<dyn KnowledgeRepository>,
        authorizer: Arc<dyn PolicyAuthorizer>,
        ownership: Option<Arc<dyn CodeBindingRepository>>,
        graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    ) -> Self {
        Self {
            memory,
            beliefs,
            knowledge,
            authorizer,
            ownership,
            graph,
            counters: Arc::new(WalkCounters::default()),
        }
    }

    /// Instrumented walk counters (work package A3): tests bound references,
    /// keyed lookups, queue/set sizes, byte accounting, exhaustions and
    /// cycle back-edges against these counters, never elapsed time.
    pub fn walk_counters(&self) -> Arc<WalkCounters> {
        Arc::clone(&self.counters)
    }

    /// Resolves graph-owned records through their canonical graph policy.
    /// Graphless entities and relationships are resolved ordinary facts whose
    /// domain default still flows through the configured owning authorizer.
    /// Missing graph authority, a missing graph row, or a foreign graph deny.
    async fn authorize_graph(
        &self,
        request: &RetrievalRequest,
        scope: &engram_domain::Scope,
        graph_id: Option<&engram_domain::KnowledgeGraphId>,
    ) -> CoreResult<bool> {
        let Some(graph_id) = graph_id else {
            return Ok(self.authorize(request, scope, &Policy::graph_lane_default()));
        };
        let Some(graphs) = &self.graph else {
            return Ok(false);
        };
        let Some(graph) = graphs.get_graph(graph_id, scope).await? else {
            return Ok(false);
        };
        if graph.scope != *scope {
            return Ok(false);
        }
        Ok(self.authorize(request, &graph.scope, &graph.policy))
    }

    /// The owning retrieval decision for one canonical record: the request's
    /// authenticated requester against the record's own scope and policy.
    fn authorize(
        &self,
        request: &RetrievalRequest,
        scope: &engram_domain::Scope,
        policy: &Policy,
    ) -> bool {
        self.authorizer
            .can_retrieve(&request.requester, scope, policy)
            .is_ok()
    }

    /// True when `record_id` carries durable code lineage in the request
    /// scope — resolved through the BOUNDED KEYED ownership read (work
    /// package A3), never a per-record full-scope lineage materialization.
    /// An unconfigured ownership reader cannot establish the negative and
    /// callers treat that as unsettled (never as a negative lookup).
    async fn in_code_lineage(
        &self,
        record_id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<bool>> {
        let Some(ownership) = &self.ownership else {
            return Ok(None);
        };
        self.counters
            .ownership_lookups
            .fetch_add(1, Ordering::Relaxed);
        Ok(Some(
            ownership
                .record_lineage(&request.scope, record_id)
                .await?
                .is_some(),
        ))
    }

    /// Own-identity classification for one record: kind-code is decided
    /// without a lookup; otherwise the keyed durable-lineage read decides.
    async fn own_identity(
        &self,
        record_id: &str,
        request: &RetrievalRequest,
        kind_is_code: bool,
    ) -> CoreResult<OwnIdentity> {
        if kind_is_code {
            return Ok(OwnIdentity::Code);
        }
        match self.in_code_lineage(record_id, request).await? {
            Some(true) => Ok(OwnIdentity::Code),
            Some(false) => Ok(OwnIdentity::Ordinary),
            None => Ok(OwnIdentity::Unsettled),
        }
    }

    /// Terminal ADMISSION exhaustion (finding 5 instrumentation): a frame
    /// that cannot fit the shared budgets ends the classification before
    /// the walk starts, with the persistent state's byte charges accounted
    /// and the exhaustion counter attesting it.
    fn admission_exhausted(&self, state: &WalkState) -> EvidenceClassification {
        self.counters
            .budget_exhaustions
            .fetch_add(1, Ordering::Relaxed);
        self.account_bytes(state);
        EvidenceClassification::incomplete(CandidateEvidence::Unknown)
    }

    /// Records the walk's TOTAL byte charges (finding 5: ALL allocation
    /// charges are instrumented — yielded references, visited keys and
    /// code-dependency ids — not only yielded-reference copies).
    fn account_bytes(&self, state: &WalkState) {
        self.counters
            .bytes_accounted
            .fetch_add(state.bytes, Ordering::Relaxed);
    }

    /// Enqueues one continuation frame against the DEPTH and BYTE budgets
    /// BEFORE it enters the queue; `false` means terminal exhaustion. The
    /// frame's admission work is ALREADY inside the persistent
    /// `state.work` (the constructors charge the ONE shared budget item by
    /// item), so only the byte reservation is re-checked here against
    /// bytes consumed since construction — re-checking work would
    /// double-count the same items.
    fn enqueue(&self, frame: Frame, stack: &mut Vec<Frame>, state: &mut WalkState) -> bool {
        if frame.depth() >= MAX_DEPENDENCY_DEPTH {
            return false;
        }
        if state.bytes + frame.remaining_bytes() > MAX_DEPENDENCY_BYTES {
            return false;
        }
        self.counters
            .frames_enqueued
            .fetch_add(1, Ordering::Relaxed);
        stack.push(frame);
        let depth = stack.len();
        self.counters
            .max_queue_depth
            .fetch_max(depth, Ordering::Relaxed);
        true
    }

    /// Bounded, cycle-safe dependency evaluation over canonical records.
    ///
    /// The walk is ITERATIVE (an explicit frame stack — async recursion
    /// would require boxing and an unbounded future). Every budget (depth,
    /// records, references, bytes) is checked BEFORE any cloning, insertion
    /// or enqueueing — ONE shared, monotonically consumed admission-work
    /// budget spans ALL frame constructors, sibling frames, flattening and
    /// frame-pop probes (finding 5); exhaustion is TERMINAL and produces an
    /// explicitly incomplete Unknown. Visited keys break cycles: a back-edge adds no
    /// new evidence (the cycle's other edge settles the fold). Code findings
    /// never terminate the walk — every reachable sibling is still
    /// resolved, and Unknown dominates the aggregate so a stale, denied or
    /// missing sibling cannot hide behind an eligible code member.
    async fn walk(
        &self,
        mut state: WalkState,
        initial: Vec<Frame>,
        request: &RetrievalRequest,
    ) -> CoreResult<EvidenceClassification> {
        // The state is the CALLER's persistent state: the initial frames'
        // admission charges are already inside `state.work`, so the ONE
        // shared budget spans frame construction AND the walk itself.
        let mut stack: Vec<Frame> = Vec::new();
        for frame in initial {
            if !self.enqueue(frame, &mut stack, &mut state) {
                self.counters
                    .budget_exhaustions
                    .fetch_add(1, Ordering::Relaxed);
                self.account_bytes(&state);
                return Ok(state.exhausted());
            }
        }
        while let Some(mut frame) = stack.pop() {
            // The frame-pop probe is STRUCTURAL work on the same shared
            // budget (finding 5): every advance-loop entry is chargeable.
            if !state.charge_admission(&self.counters) {
                self.counters
                    .budget_exhaustions
                    .fetch_add(1, Ordering::Relaxed);
                self.account_bytes(&state);
                return Ok(state.exhausted());
            }
            let frame_depth = frame.depth();
            loop {
                // Safety invariant (canonical 6d): constructors, flattening
                // and probes bound total admission work to the shared
                // budget, so this guard can only fire on a defect — but it
                // keeps the walk's terminal-exhaustion contract total.
                if state.work > MAX_DEPENDENCY_REFERENCES {
                    self.counters
                        .budget_exhaustions
                        .fetch_add(1, Ordering::Relaxed);
                    self.account_bytes(&state);
                    return Ok(state.exhausted());
                }
                self.counters
                    .references_examined
                    .fetch_add(1, Ordering::Relaxed);
                match frame.advance(&mut state, &self.counters) {
                    Advance::Done => break,
                    Advance::Rejected => {
                        self.counters
                            .budget_exhaustions
                            .fetch_add(1, Ordering::Relaxed);
                        self.account_bytes(&state);
                        return Ok(state.exhausted());
                    }
                    // ID-less or canonically unresolvable references are
                    // decisive unresolved evidence, never ordinary
                    // (canonical 6a).
                    Advance::Unresolvable => {
                        state.aggregate = state.aggregate.fold(Aggregate::Unknown);
                    }
                    Advance::Reference(target_type, id) => {
                        let (member, continuations) = self
                            .reference_evidence(
                                target_type,
                                &id,
                                frame_depth + 1,
                                request,
                                &mut state,
                            )
                            .await?;
                        match member {
                            MemberEvidence::Identity => {}
                            MemberEvidence::Ordinary => {
                                state.aggregate = state.aggregate.fold(Aggregate::Identity);
                            }
                            MemberEvidence::Code => {
                                state.aggregate = state.aggregate.fold(Aggregate::Code);
                            }
                            MemberEvidence::Unresolved => {
                                state.aggregate = state.aggregate.fold(Aggregate::Unknown);
                            }
                            MemberEvidence::Unsettled => {
                                state.aggregate = state.aggregate.fold(Aggregate::Unknown);
                                state.unsettled = true;
                            }
                            MemberEvidence::Exhausted => {
                                self.counters
                                    .budget_exhaustions
                                    .fetch_add(1, Ordering::Relaxed);
                                self.account_bytes(&state);
                                return Ok(state.exhausted());
                            }
                        }
                        for continuation in continuations {
                            if !self.enqueue(continuation, &mut stack, &mut state) {
                                self.counters
                                    .budget_exhaustions
                                    .fetch_add(1, Ordering::Relaxed);
                                self.account_bytes(&state);
                                return Ok(state.exhausted());
                            }
                        }
                        // Do not stop at a code dependency: every reachable
                        // sibling is resolved before the walk finishes.
                    }
                }
            }
            let visited = state.resolved.len();
            self.counters
                .visited_records
                .fetch_max(visited, Ordering::Relaxed);
        }
        self.account_bytes(&state);
        Ok(state.finish())
    }

    /// Classifies one walked reference by resolving its canonical record:
    /// authorization, OWN-IDENTITY classification (kind + keyed durable
    /// lineage) and the record's typed-origin continuations. Code-owned
    /// records still return their dependency continuations — a code finding
    /// never terminates resolution.
    #[allow(clippy::type_complexity)]
    async fn reference_evidence(
        &self,
        target_type: EvidenceTargetType,
        id: &str,
        depth: usize,
        request: &RetrievalRequest,
        state: &mut WalkState,
    ) -> CoreResult<(MemberEvidence, Vec<Frame>)> {
        // Byte budget BEFORE the visited-key allocation (finding 5): the
        // key's id bytes are charged before the String is ever cloned.
        if state.bytes + id.len() + 1 > MAX_DEPENDENCY_BYTES {
            return Ok((MemberEvidence::Exhausted, Vec::new()));
        }
        let key = (target_discriminant(&target_type), id.to_owned());
        if state.resolved.contains(&key) {
            // Cycle back-edge: the record's classification is already being
            // settled elsewhere in this walk; contribute nothing (the
            // monoid identity).
            self.counters
                .cycle_backedges
                .fetch_add(1, Ordering::Relaxed);
            return Ok((MemberEvidence::Identity, Vec::new()));
        }
        if state.resolved.len() >= MAX_DEPENDENCY_RECORDS {
            return Ok((MemberEvidence::Exhausted, Vec::new()));
        }
        if !state.admit_visited(key) {
            return Ok((MemberEvidence::Exhausted, Vec::new()));
        }
        match target_type {
            // Code-capable record categories: the reference must resolve to
            // an authorized canonical record, which is then classified by
            // its own kind, durable lineage and dependencies — INCLUDING
            // its source's provenance (typed origins, work package A3).
            EvidenceTargetType::Chunk => {
                let Some(record) = self
                    .knowledge
                    .get_chunk(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                };
                // Chunks authorize through BOTH their canonical source's
                // scope/policy AND their own policy (canonical 6a).
                let Some(source) = self
                    .knowledge
                    .get_source(&record.source_id, &request.scope)
                    .await?
                else {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                };
                if !self.authorize(request, &source.scope, &source.policy)
                    || !self.authorize(request, &source.scope, &record.policy)
                {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                }
                let member = match self
                    .own_identity(id, request, code_chunk(record.kind))
                    .await?
                {
                    OwnIdentity::Code => {
                        if !state.admit_code_dependency(id) {
                            return Ok((MemberEvidence::Exhausted, Vec::new()));
                        }
                        MemberEvidence::Code
                    }
                    OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                    OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                };
                // Source + chunk dependencies (work package A3 + review r5
                // finding 7): the canonical SOURCE is processed as a bounded
                // TYPED SOURCE dependency — its own identity/lineage settles
                // through the keyed lineage read (a repository-bound source
                // without settled ownership fails closed) and its provenance
                // walks the canonical path — not a provenance copy that
                // skipped the source's own ownership classification. The
                // synthetic source reference's id is budget-checked BEFORE
                // its String is allocated (finding 5 — verification
                // finding 3).
                if !state.admits_synthetic_ref(&self.counters, record.source_id.as_str().len()) {
                    return Ok((MemberEvidence::Exhausted, Vec::new()));
                }
                let chunk_frame =
                    Frame::provenance(record.provenance, depth, state, &self.counters);
                let source_frame = Frame::refs(
                    vec![source_dependency_ref(&record.source_id)],
                    depth,
                    state,
                    &self.counters,
                );
                match (chunk_frame, source_frame) {
                    (Some(chunk_frame), Some(source_frame)) => {
                        Ok((member, vec![chunk_frame, source_frame]))
                    }
                    _ => Ok((MemberEvidence::Exhausted, Vec::new())),
                }
            }
            EvidenceTargetType::Entity => {
                let Some(record) = self
                    .knowledge
                    .get_entity(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                };
                if !self
                    .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                    .await?
                {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                }
                let member = match self
                    .own_identity(id, request, entity_is_code_kind(&record))
                    .await?
                {
                    OwnIdentity::Code => {
                        if !state.admit_code_dependency(id) {
                            return Ok((MemberEvidence::Exhausted, Vec::new()));
                        }
                        MemberEvidence::Code
                    }
                    OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                    OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                };
                // Provenance AND typed `source_refs` (work package A3), with
                // budget-aware admission scans (finding 5).
                let provenance_frame =
                    Frame::provenance(record.provenance, depth, state, &self.counters);
                let refs_frame = Frame::refs(record.source_refs, depth, state, &self.counters);
                match (provenance_frame, refs_frame) {
                    (Some(provenance_frame), Some(refs_frame)) => {
                        Ok((member, vec![provenance_frame, refs_frame]))
                    }
                    _ => Ok((MemberEvidence::Exhausted, Vec::new())),
                }
            }
            // Sources: after source-policy authorization the record's own
            // identity settles through the keyed lineage read, and its
            // canonical provenance WALKS (work package A3 — a source is no
            // longer a terminal origin). A repository-bound source outside
            // the durable catalog cannot be positively ordinary.
            EvidenceTargetType::Source => {
                let Some(record) = self
                    .knowledge
                    .get_source(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                };
                if !self.authorize(request, &record.scope, &record.policy) {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                }
                if record.repository_key.is_some() {
                    return match self.in_code_lineage(id, request).await? {
                        Some(true) => {
                            if !state.admit_code_dependency(id) {
                                return Ok((MemberEvidence::Exhausted, Vec::new()));
                            }
                            let provenance_frame =
                                Frame::provenance(record.provenance, depth, state, &self.counters);
                            match provenance_frame {
                                Some(provenance_frame) => {
                                    Ok((MemberEvidence::Code, vec![provenance_frame]))
                                }
                                None => Ok((MemberEvidence::Exhausted, Vec::new())),
                            }
                        }
                        // Repository-bound but outside the durable lineage
                        // catalog, or the reader is unconfigured: unsettled
                        // code ownership is never ordinary.
                        Some(false) | None => Ok((MemberEvidence::Unsettled, Vec::new())),
                    };
                }
                let member = match self.own_identity(id, request, false).await? {
                    OwnIdentity::Code => {
                        if !state.admit_code_dependency(id) {
                            return Ok((MemberEvidence::Exhausted, Vec::new()));
                        }
                        MemberEvidence::Code
                    }
                    OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                    OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                };
                let provenance_frame =
                    Frame::provenance(record.provenance, depth, state, &self.counters);
                match provenance_frame {
                    Some(provenance_frame) => Ok((member, vec![provenance_frame])),
                    None => Ok((MemberEvidence::Exhausted, Vec::new())),
                }
            }
            // Relationship references resolve canonically through the graph
            // port; classification continues through the relationship's own
            // provenance AND evidence refs after its own-identity check.
            // Review r5 (finding 4, adjudicated REQUIRED): the subject and
            // object EntityRef endpoints are TYPED IDENTITY — they resolve
            // through the bounded canonical Entity path (their actual owners
            // authorize, their own lineage classifies), and an ID-bearing
            // endpoint that cannot resolve fails closed; an ID-less
            // endpoint is unresolvable identity, never ordinary.
            EvidenceTargetType::Relationship => {
                let Some(record) = self
                    .knowledge
                    .get_relationship(&(id.to_owned().into()), &request.scope)
                    .await?
                else {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                };
                if !self
                    .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                    .await?
                {
                    return Ok((MemberEvidence::Unresolved, Vec::new()));
                }
                let member = match self.own_identity(id, request, false).await? {
                    OwnIdentity::Code => {
                        if !state.admit_code_dependency(id) {
                            return Ok((MemberEvidence::Exhausted, Vec::new()));
                        }
                        MemberEvidence::Code
                    }
                    OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                    OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                };
                // The synthetic ENDPOINT references allocate owned ids
                // from the borrowed EntityRefs; each ID-bearing endpoint is
                // budget-checked BEFORE its String is allocated (finding 5
                // — verification finding 3). An ID-less endpoint allocates
                // nothing and stays unresolvable identity.
                for endpoint in [&record.subject, &record.object] {
                    if let Some(id) = endpoint.id.as_ref() {
                        if !state.admits_synthetic_ref(&self.counters, id.as_str().len()) {
                            return Ok((MemberEvidence::Exhausted, Vec::new()));
                        }
                    }
                }
                let provenance_frame =
                    Frame::provenance(record.provenance, depth, state, &self.counters);
                let evidence_frame = Frame::refs(record.evidence, depth, state, &self.counters);
                let endpoints_frame = Frame::refs(
                    vec![endpoint_ref(&record.subject), endpoint_ref(&record.object)],
                    depth,
                    state,
                    &self.counters,
                );
                match (provenance_frame, evidence_frame, endpoints_frame) {
                    (Some(provenance_frame), Some(evidence_frame), Some(endpoints_frame)) => Ok((
                        member,
                        vec![provenance_frame, evidence_frame, endpoints_frame],
                    )),
                    _ => Ok((MemberEvidence::Exhausted, Vec::new())),
                }
            }
            // Derived-record categories: resolve the canonical record,
            // classify its OWN lineage (work package A3 — Memory and Belief
            // are own-lineage classified too), and continue the walk through
            // its provenance (and typed belief origins). An ID-less derived
            // reference is unresolved evidence, never ordinary (canonical
            // 6a).
            EvidenceTargetType::Memory | EvidenceTargetType::Belief => match target_type {
                EvidenceTargetType::Memory => {
                    let Some(record) = self
                        .memory
                        .get_memory(&(id.to_owned().into()), &request.scope)
                        .await?
                    else {
                        return Ok((MemberEvidence::Unresolved, Vec::new()));
                    };
                    if !self.authorize(request, &record.scope, &record.policy) {
                        return Ok((MemberEvidence::Unresolved, Vec::new()));
                    }
                    let member = match self.own_identity(id, request, false).await? {
                        OwnIdentity::Code => {
                            if !state.admit_code_dependency(id) {
                                return Ok((MemberEvidence::Exhausted, Vec::new()));
                            }
                            MemberEvidence::Code
                        }
                        OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                        OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                    };
                    let provenance_frame =
                        Frame::provenance(record.provenance, depth, state, &self.counters);
                    match provenance_frame {
                        Some(provenance_frame) => Ok((member, vec![provenance_frame])),
                        None => Ok((MemberEvidence::Exhausted, Vec::new())),
                    }
                }
                _ => {
                    let Some(record) = self
                        .beliefs
                        .get_belief_by_id(&(id.to_owned().into()), &request.scope)
                        .await?
                    else {
                        return Ok((MemberEvidence::Unresolved, Vec::new()));
                    };
                    if !self.authorize(request, &record.scope, &record.policy) {
                        return Ok((MemberEvidence::Unresolved, Vec::new()));
                    }
                    let member = match self.own_identity(id, request, false).await? {
                        OwnIdentity::Code => {
                            if !state.admit_code_dependency(id) {
                                return Ok((MemberEvidence::Exhausted, Vec::new()));
                            }
                            MemberEvidence::Code
                        }
                        OwnIdentity::Ordinary => MemberEvidence::Ordinary,
                        OwnIdentity::Unsettled => MemberEvidence::Unsettled,
                    };
                    // Typed belief dependencies (canonical 6b): the
                    // belief's own provenance, its `sources` and the
                    // synthesizer derivation's `input_refs` all walk —
                    // moved, never synthetically cloned.
                    let provenance_frame =
                        Frame::provenance(record.provenance, depth, state, &self.counters);
                    let sources_frame =
                        Frame::belief_sources(record.sources, depth, state, &self.counters);
                    let synthesizer_frame = record.synthesizer.map(|synthesizer| {
                        Frame::refs(synthesizer.input_refs, depth, state, &self.counters)
                    });
                    match (provenance_frame, sources_frame, synthesizer_frame) {
                        (Some(provenance_frame), Some(sources_frame), synthesizer_frame) => {
                            let mut continuations = vec![provenance_frame, sources_frame];
                            match synthesizer_frame {
                                // The synthesizer refs could not be admitted.
                                Some(None) => return Ok((MemberEvidence::Exhausted, Vec::new())),
                                Some(Some(synthesizer_frame)) => {
                                    continuations.push(synthesizer_frame)
                                }
                                None => {}
                            }
                            Ok((member, continuations))
                        }
                        _ => Ok((MemberEvidence::Exhausted, Vec::new())),
                    }
                }
            },
            // Canonically unresolvable categories: documents have no scoped
            // canonical read on the knowledge port and concepts/events/urls
            // carry no canonical code association — unresolved evidence
            // degrades to Unknown, never Ordinary (canonical 6a). (The
            // frame layer already maps these to `Unresolvable`; this arm
            // keeps the classifier total for direct callers.)
            EvidenceTargetType::Document
            | EvidenceTargetType::Concept
            | EvidenceTargetType::Event
            | EvidenceTargetType::Url => Ok((MemberEvidence::Unresolved, Vec::new())),
        }
    }
}

/// Deterministic lane projections (work package A3): retrieval lanes publish
/// a DETERMINISTIC projection of the canonical record, so the anti-forgery
/// payload binding accepts EXACTLY those projections while any other content
/// still denies. The candidate's target id binds identity and the payload
/// equality binds the content revision.
fn entity_content_matches(record: &engram_domain::KnowledgeEntity, content: &str) -> bool {
    if record.name == content {
        return true;
    }
    // The SQLite lexical lane indexes `"{name} {kind:?}"` (mirroring what
    // `scan_repo` indexed).
    format!("{} {:?}", record.name, record.kind) == content
}

/// Chunk projection binding (review r5, finding 9): each deterministic
/// lane projection is evaluated INDEPENDENTLY — the full text (SQLite
/// lanes), the pgvector graph lane's 2000-char clip and the pgvector vector
/// lane's 2048-char cap. No length pre-check may skip a projection: a
/// 2,001-character canonical chunk whose graph lane legitimately returns
/// its first 2,000 characters still binds. All projections take CHARACTERS
/// (`.chars().take(n)`), matching the lanes.
fn chunk_content_matches(record: &engram_domain::KnowledgeChunk, content: &str) -> bool {
    if record.text == content {
        return true;
    }
    // Clipping only differs from the full text beyond the clip point; each
    // projection is tested regardless of the canonical text's byte length.
    let clipped: String = record.text.chars().take(2000).collect();
    if clipped == content {
        return true;
    }
    let capped: String = record.text.chars().take(2048).collect();
    capped == content
}

fn code_chunk(kind: KnowledgeChunkKind) -> bool {
    matches!(
        kind,
        KnowledgeChunkKind::CodeSymbol
            | KnowledgeChunkKind::CodeBlock
            | KnowledgeChunkKind::DiffHunk
    )
}

/// One canonical SOURCE dependency reference (review r5, finding 7): the
/// chunk's source id as a typed reference so the source's OWN identity,
/// lineage and provenance are processed through the bounded canonical
/// Source path — a provenance copy alone never settles the source's
/// ownership.
fn source_dependency_ref(source_id: &engram_domain::SourceId) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Source,
        target_id: Some(source_id.to_string()),
        uri: None,
        quote: None,
        location: None,
    }
}

/// One relationship endpoint as a canonical Entity dependency (review r5,
/// finding 4): ID-bearing endpoints resolve through the bounded canonical
/// Entity path — their actual owners authorize and their own durable
/// lineage classifies. An ID-less endpoint yields an ID-less typed
/// reference, which the frame layer treats as UNRESOLVABLE identity: it can
/// never finish Ordinary.
fn endpoint_ref(endpoint: &engram_domain::EntityRef) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: endpoint.id.as_ref().map(|id| id.to_string()),
        uri: None,
        quote: None,
        location: None,
    }
}

/// Symbol/structural entity kinds. An entity of a code kind is code even
/// without durable lineage (legacy unbound code) and is denied while the
/// binding authority is unavailable.
fn entity_is_code_kind(entity: &engram_domain::KnowledgeEntity) -> bool {
    matches!(
        entity.kind,
        engram_domain::EntityKind::Class
            | engram_domain::EntityKind::Function
            | engram_domain::EntityKind::Method
            | engram_domain::EntityKind::Struct
            | engram_domain::EntityKind::Interface
            | engram_domain::EntityKind::Trait
            | engram_domain::EntityKind::TypeAlias
            | engram_domain::EntityKind::Enum
    )
}

/// Merges the candidate's OWN-IDENTITY classification into the walk result
/// (review r5, finding 1): own code joins the code-dependency set, an
/// unsettled own lineage leaves the classification incomplete no matter
/// what the walk found, and an UNRESOLVED walk stays Unknown regardless of
/// own-code identity — own identity APPENDS to a resolved Ordinary or
/// CodeDependent result, it never weakens the aggregate.
fn merge_own(
    classification: EvidenceClassification,
    own: OwnIdentity,
    own_id: &str,
) -> EvidenceClassification {
    match own {
        OwnIdentity::Ordinary => classification,
        OwnIdentity::Unsettled => EvidenceClassification::incomplete(CandidateEvidence::Unknown),
        OwnIdentity::Code => {
            if !classification.complete {
                // Terminal exhaustion or an unsettled member already denies
                // incompletely; own code cannot restore completeness.
                return classification;
            }
            match classification.evidence {
                // An unresolved (or aggregate-Unknown) walk stays dominant:
                // the walk's denial — a missing, denied or foreign
                // dependency — must not be discarded by own-code identity.
                CandidateEvidence::Unknown => classification,
                CandidateEvidence::Ordinary | CandidateEvidence::CodeDependent => {
                    let mut code_dependencies = classification.code_dependencies;
                    code_dependencies.push(own_id.to_owned());
                    EvidenceClassification::complete(
                        CandidateEvidence::CodeDependent,
                        code_dependencies,
                    )
                }
            }
        }
    }
}

#[async_trait]
impl CandidateEvidenceResolver for RepositoryCandidateEvidence {
    async fn evidence(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<CandidateEvidence> {
        Ok(self
            .classify_with_dependencies(candidate, request)
            .await?
            .evidence)
    }

    /// Full classification with code dependency ids (canonical 6c): recipes
    /// with a SUCCESSFUL view must still validate derived records' code
    /// dependencies against that view. Every candidate kind — Memory and
    /// Belief included — is own-lineage classified and fully walked before
    /// any positive classification (work package A3).
    async fn classify_with_dependencies(
        &self,
        candidate: &RetrievalResult,
        request: &RetrievalRequest,
    ) -> CoreResult<EvidenceClassification> {
        let unbound = || EvidenceClassification::incomplete(CandidateEvidence::Unknown);
        // ONE persistent walk state (finding 5 — verification finding 3):
        // the initial frames' admission charges AND the candidate's own-id
        // charge consume the SAME shared budget the walk continues to
        // consume; a frame or charge that cannot fit is terminal ADMISSION
        // exhaustion — distinct from an unbound candidate only in cause,
        // identical in shape, and counted.
        let mut state = WalkState::default();
        match candidate.target_type {
            RetrievalTargetType::Memory => match self
                .memory
                .get_memory(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self.authorize(request, &record.scope, &record.policy)
                        && record.content.text == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    let own = self
                        .own_identity(candidate.target_id.as_str(), request, false)
                        .await?;
                    if matches!(own, OwnIdentity::Code)
                        && !state.charge_own_id(&self.counters, candidate.target_id.as_str())
                    {
                        return Ok(self.admission_exhausted(&state));
                    }
                    let walked =
                        match Frame::provenance(record.provenance, 0, &mut state, &self.counters) {
                            Some(frame) => self.walk(state, vec![frame], request).await?,
                            None => return Ok(self.admission_exhausted(&state)),
                        };
                    Ok(merge_own(walked, own, candidate.target_id.as_str()))
                }
                Some(_) | None => Ok(unbound()),
            },
            RetrievalTargetType::Belief => match self
                .beliefs
                .get_belief_by_id(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if self.authorize(request, &record.scope, &record.policy)
                        && record.content == candidate.content
                        && record.provenance == candidate.provenance
                        && record.policy == candidate.policy =>
                {
                    let own = self
                        .own_identity(candidate.target_id.as_str(), request, false)
                        .await?;
                    if matches!(own, OwnIdentity::Code)
                        && !state.charge_own_id(&self.counters, candidate.target_id.as_str())
                    {
                        return Ok(self.admission_exhausted(&state));
                    }
                    // Typed belief dependencies (canonical 6b): provenance,
                    // sources and synthesizer inputs all walk — moved, never
                    // synthetically cloned.
                    let provenance_frame =
                        Frame::provenance(record.provenance, 0, &mut state, &self.counters);
                    let sources_frame =
                        Frame::belief_sources(record.sources, 0, &mut state, &self.counters);
                    let synthesizer_frame = record.synthesizer.map(|synthesizer| {
                        Frame::refs(synthesizer.input_refs, 0, &mut state, &self.counters)
                    });
                    let mut initial = Vec::with_capacity(3);
                    match (provenance_frame, sources_frame, synthesizer_frame) {
                        (Some(provenance_frame), Some(sources_frame), synthesizer_frame) => {
                            initial.push(provenance_frame);
                            initial.push(sources_frame);
                            match synthesizer_frame {
                                Some(Some(frame)) => initial.push(frame),
                                // The synthesizer refs could not be admitted.
                                Some(None) => return Ok(self.admission_exhausted(&state)),
                                None => {}
                            }
                        }
                        _ => return Ok(self.admission_exhausted(&state)),
                    }
                    let walked = self.walk(state, initial, request).await?;
                    Ok(merge_own(walked, own, candidate.target_id.as_str()))
                }
                Some(_) | None => Ok(unbound()),
            },
            RetrievalTargetType::Chunk => match self
                .knowledge
                .get_chunk(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record) => match self
                    .knowledge
                    .get_source(&record.source_id, &request.scope)
                    .await?
                {
                    Some(source)
                        if self.authorize(request, &source.scope, &source.policy)
                            && self.authorize(request, &source.scope, &record.policy)
                            && chunk_content_matches(&record, &candidate.content)
                            && record.provenance == candidate.provenance
                            && record.policy == candidate.policy =>
                    {
                        let own = self
                            .own_identity(
                                candidate.target_id.as_str(),
                                request,
                                code_chunk(record.kind),
                            )
                            .await?;
                        if matches!(own, OwnIdentity::Code)
                            && !state.charge_own_id(&self.counters, candidate.target_id.as_str())
                        {
                            return Ok(self.admission_exhausted(&state));
                        }
                        // Chunk + SOURCE dependencies (work package A3 +
                        // review r5 finding 7): the canonical source is a
                        // bounded TYPED SOURCE dependency — its own
                        // identity/lineage and provenance process through
                        // the canonical path, not a provenance copy. The
                        // synthetic source reference's id is budget-checked
                        // BEFORE its String is allocated (finding 5).
                        if !state
                            .admits_synthetic_ref(&self.counters, record.source_id.as_str().len())
                        {
                            return Ok(self.admission_exhausted(&state));
                        }
                        let chunk_frame =
                            Frame::provenance(record.provenance, 0, &mut state, &self.counters);
                        let source_frame = Frame::refs(
                            vec![source_dependency_ref(&record.source_id)],
                            0,
                            &mut state,
                            &self.counters,
                        );
                        let walked = match (chunk_frame, source_frame) {
                            (Some(chunk_frame), Some(source_frame)) => {
                                self.walk(state, vec![chunk_frame, source_frame], request)
                                    .await?
                            }
                            _ => return Ok(self.admission_exhausted(&state)),
                        };
                        Ok(merge_own(walked, own, candidate.target_id.as_str()))
                    }
                    Some(_) | None => Ok(unbound()),
                },
                None => Ok(unbound()),
            },
            RetrievalTargetType::Entity => match self
                .knowledge
                .get_entity(&candidate.target_id.clone().into(), &request.scope)
                .await?
            {
                Some(record)
                    if entity_content_matches(&record, &candidate.content)
                        && record.provenance == candidate.provenance =>
                {
                    if !self
                        .authorize_graph(request, &record.scope, record.graph_id.as_ref())
                        .await?
                    {
                        return Ok(unbound());
                    }
                    let own = self
                        .own_identity(
                            candidate.target_id.as_str(),
                            request,
                            entity_is_code_kind(&record),
                        )
                        .await?;
                    if matches!(own, OwnIdentity::Code)
                        && !state.charge_own_id(&self.counters, candidate.target_id.as_str())
                    {
                        return Ok(self.admission_exhausted(&state));
                    }
                    // Provenance AND typed `source_refs` (work package A3).
                    let provenance_frame =
                        Frame::provenance(record.provenance, 0, &mut state, &self.counters);
                    let refs_frame = Frame::refs(record.source_refs, 0, &mut state, &self.counters);
                    let walked = match (provenance_frame, refs_frame) {
                        (Some(provenance_frame), Some(refs_frame)) => {
                            self.walk(state, vec![provenance_frame, refs_frame], request)
                                .await?
                        }
                        _ => return Ok(self.admission_exhausted(&state)),
                    };
                    Ok(merge_own(walked, own, candidate.target_id.as_str()))
                }
                Some(_) | None => Ok(unbound()),
            },
            _ => Ok(unbound()),
        }
    }
}
