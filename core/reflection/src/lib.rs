//! Reflection operator — abstracts scoped active memories into derived beliefs.
//!
//! The memory dimension engram covers least (reflection / abstraction; today
//! consolidation *compresses* via compaction/decay but does not *abstract*).
//! This crate fills two empty slots: a production [`BeliefSynthesizer`] (the
//! reflection synthesizer, deterministic baseline) and a production
//! [`ConsolidationMutationExecutor`] (dispatches `BeliefSynthesis` → synthesizer
//! → `BeliefRepository::put_belief`). Zero contract change — reuses the
//! declared-but-unimplemented `BeliefSynthesizer` trait, the `BeliefSynthesis`
//! task kind, and free-form `provenance.method = "reflection"`.
//!
//! The real LLM impl is deferred behind the trait (deterministic baseline
//! in-tree). Production wiring is a follow-up — it needs a composite-executor
//! pattern (`Hybrid` bundles 8 task kinds; a single-purpose executor alone
//! would skip 7).

mod belief_build;
mod executor;
mod source;
mod synthesizer;

pub use executor::ReflectionExecutor;
pub use source::{ActiveMemorySource, BeliefSink};
pub use synthesizer::ReflectionSynthesizer;
