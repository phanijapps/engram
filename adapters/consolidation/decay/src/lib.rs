//! Decay consolidation executor — restores the retired policy-expiry behavior.
//!
//! Handles `ConsolidationTaskKind::Decay`: marks in-scope active memories whose
//! `policy.expires_at` is past as `MemoryStatus::Expired`, skips
//! `Retention::LegalHold`, and reports `records_decayed`. Other task kinds are
//! `Skipped`. Designed to compose with `ReflectionExecutor` via
//! `CompositeConsolidationExecutor`.
//!
//! The Ebbinghaus forgetting curve (`R = e^(-t/S)`, from `engram-consolidation`)
//! is used to rank due records by retention loss — records with the lowest R
//! (most forgotten) are decayed first when a budget applies.

mod executor;
mod source;

pub use executor::DecayExecutor;
pub use source::{DecayCandidate, DecayMemorySource};
