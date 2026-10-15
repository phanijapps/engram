use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Admission happens before tokenization, including embedded struct callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "QueryWire")]
pub struct CodeEvidenceQuery {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_per_lane: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_token_budget: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CodeEvidenceValidationError {
    #[error("query must be nonempty and at most 16384 UTF-8 bytes")]
    Query,
    #[error("repository must be nonempty and at most 2048 UTF-8 bytes")]
    Repository,
    #[error("resultLimit must be in 1..=100")]
    ResultLimit,
    #[error("candidatesPerLane must be in 10..=1000")]
    CandidatesPerLane,
    #[error("evidenceTokenBudget must be in 256..=32768")]
    EvidenceTokenBudget,
}

impl CodeEvidenceQuery {
    pub fn validate(&self) -> Result<(), CodeEvidenceValidationError> {
        if self.query.trim().is_empty() || self.query.len() > 16384 {
            return Err(CodeEvidenceValidationError::Query);
        }
        if self
            .repository
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.len() > 2048)
        {
            return Err(CodeEvidenceValidationError::Repository);
        }
        if self.result_limit.is_some_and(|v| !(1..=100).contains(&v)) {
            return Err(CodeEvidenceValidationError::ResultLimit);
        }
        if self
            .candidates_per_lane
            .is_some_and(|v| !(10..=1000).contains(&v))
        {
            return Err(CodeEvidenceValidationError::CandidatesPerLane);
        }
        if self
            .evidence_token_budget
            .is_some_and(|v| !(256..=32768).contains(&v))
        {
            return Err(CodeEvidenceValidationError::EvidenceTokenBudget);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QueryWire {
    query: String,
    repository: Option<String>,
    result_limit: Option<u32>,
    candidates_per_lane: Option<u32>,
    evidence_token_budget: Option<u32>,
}

impl TryFrom<QueryWire> for CodeEvidenceQuery {
    type Error = CodeEvidenceValidationError;
    fn try_from(w: QueryWire) -> Result<Self, Self::Error> {
        let q = Self {
            query: w.query,
            repository: w.repository,
            result_limit: w.result_limit,
            candidates_per_lane: w.candidates_per_lane,
            evidence_token_budget: w.evidence_token_budget,
        };
        q.validate()?;
        Ok(q)
    }
}
