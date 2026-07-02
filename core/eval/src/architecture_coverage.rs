//! Architecture-level evaluation coverage summaries.
//!
//! The memory fixture runner reports case pass/fail details. This module maps
//! executed cases to the architecture parity capabilities they prove, without
//! making `engram-eval` execute hierarchy, taxonomy, belief, or adapter code
//! itself. Each subsystem can produce deterministic cases and then use this
//! common coverage summary at the parity gate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::EvaluationCaseReport;

/// Architecture parity capability that an executed evaluation case can prove.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureEvalCapability {
    AcceptedRecall,
    ForbiddenRecall,
    Leakage,
    PolicyFiltering,
    Ranking,
    HierarchyGranularity,
    TaxonomyDrift,
    BeliefLifecycle,
    ContradictionReview,
    ConsolidationGate,
    AdapterReadiness,
}

/// One executed case annotated with the architecture capabilities it covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureEvalCase {
    pub case_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<ArchitectureEvalCapability>,
    pub passed: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub failures: Vec<String>,
}

impl ArchitectureEvalCase {
    /// Creates an architecture coverage case from a core evaluation case report.
    pub fn from_case_report(
        report: &EvaluationCaseReport,
        capabilities: Vec<ArchitectureEvalCapability>,
    ) -> Self {
        Self {
            case_id: report.case_id.clone(),
            capabilities,
            passed: report.passed,
            failures: report.failures.clone(),
        }
    }
}

/// Coverage result for a set of architecture-level evaluation cases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureEvalCoverage {
    pub total_required: usize,
    pub covered: Vec<ArchitectureEvalCapability>,
    pub passing: Vec<ArchitectureEvalCapability>,
    pub failing: Vec<ArchitectureEvalCapability>,
    pub missing: Vec<ArchitectureEvalCapability>,
    pub case_count: usize,
}

impl ArchitectureEvalCoverage {
    /// Returns true when every required capability is covered by passing cases.
    pub fn passed(&self) -> bool {
        self.missing.is_empty() && self.failing.is_empty()
    }
}

/// Required parity capabilities for the research architecture acceptance gate.
pub fn required_architecture_capabilities() -> Vec<ArchitectureEvalCapability> {
    vec![
        ArchitectureEvalCapability::AcceptedRecall,
        ArchitectureEvalCapability::ForbiddenRecall,
        ArchitectureEvalCapability::Leakage,
        ArchitectureEvalCapability::PolicyFiltering,
        ArchitectureEvalCapability::Ranking,
        ArchitectureEvalCapability::HierarchyGranularity,
        ArchitectureEvalCapability::TaxonomyDrift,
        ArchitectureEvalCapability::BeliefLifecycle,
        ArchitectureEvalCapability::ContradictionReview,
        ArchitectureEvalCapability::ConsolidationGate,
        ArchitectureEvalCapability::AdapterReadiness,
    ]
}

/// Summarizes which required architecture capabilities are covered and passing.
pub fn summarize_architecture_coverage(
    cases: impl IntoIterator<Item = ArchitectureEvalCase>,
    required: &[ArchitectureEvalCapability],
) -> ArchitectureEvalCoverage {
    let cases = cases.into_iter().collect::<Vec<_>>();
    let mut by_capability: BTreeMap<ArchitectureEvalCapability, Vec<&ArchitectureEvalCase>> =
        BTreeMap::new();
    for case in &cases {
        for capability in &case.capabilities {
            by_capability
                .entry(capability.clone())
                .or_default()
                .push(case);
        }
    }

    let required_set = required.iter().cloned().collect::<BTreeSet<_>>();
    let covered_set = by_capability
        .keys()
        .filter(|capability| required_set.contains(*capability))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut passing = Vec::new();
    let mut failing = Vec::new();
    let mut missing = Vec::new();

    for capability in required {
        match by_capability.get(capability) {
            None => missing.push(capability.clone()),
            Some(cases) if cases.iter().all(|case| case.passed) => passing.push(capability.clone()),
            Some(_) => failing.push(capability.clone()),
        }
    }

    ArchitectureEvalCoverage {
        total_required: required.len(),
        covered: covered_set.into_iter().collect(),
        passing,
        failing,
        missing,
        case_count: cases.len(),
    }
}
