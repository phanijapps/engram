//! Additive source-grounded evidence DTOs; no inference or storage behavior.
mod coverage;
mod decision;
mod packet;
mod query;
mod source;

pub use coverage::CodeTokenCoverage;
pub use decision::{CodeParentDecision, CodeParentDecisionReason};
pub use packet::*;
pub use query::{CodeEvidenceQuery, CodeEvidenceValidationError};
pub use source::CodeEvidenceSourceRef;

/// Required nullable wire values preserve explicit unknowns instead of omission.
pub(crate) fn required_nullable<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(de)
}
