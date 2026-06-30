//! Deterministic evaluation harness for Engram memory implementations.
//!
//! This crate executes portable `EvaluationFixture` contracts against a
//! `MemoryService`. It does not own storage or retrieval behavior; it only
//! seeds setup data through normal writes, runs retrieval cases, and reports
//! quality or policy failures in a stable format.

use std::sync::Arc;

use async_trait::async_trait;
use engram_core::{
    CoreResult, EvaluationCaseReport, EvaluationReport, EvaluationRunner, MemoryService,
};
use engram_domain::*;

pub mod accepted_examples;
mod contract_runner;
mod report_summary;

pub use contract_runner::{MemoryContractRunner, RetrievalContractOutcome};
pub use report_summary::{
    CaseReportSummary, FixtureReportSummary, FixtureSetReportSummary, summarize_report,
    summarize_reports,
};

/// Runs evaluation fixtures against a supplied memory service.
///
/// The runner is intentionally adapter-neutral. In-memory, SQL, native binding,
/// and future vector-backed implementations should be able to reuse it as long
/// as they satisfy the `MemoryService` contract.
pub struct MemoryFixtureRunner {
    service: Arc<dyn MemoryService>,
}

impl MemoryFixtureRunner {
    /// Creates a runner over a shared memory service implementation.
    ///
    /// The service is reused for setup and all cases in a fixture, so callers
    /// should provide a fresh service when they need isolated fixture state.
    pub fn new(service: Arc<dyn MemoryService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl EvaluationRunner for MemoryFixtureRunner {
    async fn run_fixture(&self, fixture: EvaluationFixture) -> CoreResult<EvaluationReport> {
        let mut aliases = Vec::new();
        for memory in fixture.setup.memories {
            let response = self.service.write_memory(memory).await?;
            aliases.push(response.record.id.to_string());
        }

        let mut reports = Vec::new();
        for case in fixture.cases {
            let context = self.service.retrieve(case.request.clone()).await?;
            reports.push(evaluate_case(case, &context, &aliases));
        }

        Ok(EvaluationReport {
            fixture_id: fixture.id,
            cases: reports,
        })
    }
}

fn evaluate_case(
    case: EvaluationCase,
    context: &ContextPayload,
    memory_aliases: &[String],
) -> EvaluationCaseReport {
    let mut failures = Vec::new();
    for expected in &case.expect.must_include {
        if !contains_target(context, expected, memory_aliases) {
            failures.push(format!(
                "missing required target {}:{}",
                target_type_name(&expected.target_type),
                expected.target_id
            ));
        }
    }
    for expected in &case.expect.must_exclude {
        if contains_target(context, expected, memory_aliases) {
            failures.push(format!(
                "forbidden target returned {}:{}",
                target_type_name(&expected.target_type),
                expected.target_id
            ));
        }
    }
    if let Some(max_results) = case.expect.max_results
        && context.items.len() > max_results as usize
    {
        failures.push(format!(
            "too many results: expected at most {max_results}, got {}",
            context.items.len()
        ));
    }
    if let Some(min_score) = case.expect.min_score {
        for item in &context.items {
            if item.score.total < min_score {
                failures.push(format!(
                    "result {} score {} below minimum {min_score}",
                    item.target_id, item.score.total
                ));
            }
        }
    }
    if case.expect.requires_explanation.unwrap_or(false) {
        for expected in &case.expect.must_include {
            if let Some(item) = matching_item(context, expected, memory_aliases)
                && item.explanation.is_none()
            {
                failures.push(format!(
                    "missing explanation for {}:{}",
                    target_type_name(&expected.target_type),
                    expected.target_id
                ));
            }
        }
    }

    EvaluationCaseReport {
        case_id: case.id,
        passed: failures.is_empty(),
        failures,
    }
}

fn contains_target(
    context: &ContextPayload,
    expected: &ExpectedTarget,
    memory_aliases: &[String],
) -> bool {
    matching_item(context, expected, memory_aliases).is_some()
}

fn matching_item<'a>(
    context: &'a ContextPayload,
    expected: &ExpectedTarget,
    memory_aliases: &[String],
) -> Option<&'a RetrievalResult> {
    context.items.iter().find(|item| {
        item.target_type == expected.target_type
            && (item.target_id == expected.target_id
                || resolves_memory_alias(&expected.target_id, &item.target_id, memory_aliases))
    })
}

fn resolves_memory_alias(expected: &str, actual: &str, memory_aliases: &[String]) -> bool {
    let Some(alias_number) = expected.strip_prefix("memory-") else {
        return false;
    };
    let Ok(alias_number) = alias_number.parse::<usize>() else {
        return false;
    };
    alias_number > 0
        && memory_aliases
            .get(alias_number - 1)
            .is_some_and(|id| id == actual)
}

fn target_type_name(target_type: &RetrievalTargetType) -> &'static str {
    match target_type {
        RetrievalTargetType::Memory => "memory",
        RetrievalTargetType::Event => "event",
        RetrievalTargetType::Chunk => "chunk",
        RetrievalTargetType::Document => "document",
        RetrievalTargetType::Entity => "entity",
        RetrievalTargetType::Relationship => "relationship",
        RetrievalTargetType::Concept => "concept",
        RetrievalTargetType::Belief => "belief",
        RetrievalTargetType::Contradiction => "contradiction",
        RetrievalTargetType::HierarchyNode => "hierarchy_node",
        RetrievalTargetType::HierarchyRelation => "hierarchy_relation",
    }
}
