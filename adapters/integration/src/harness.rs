//! Conformance harness for executing capability fixtures.
//!
//! This module provides the harness that runs fixtures for each capability
//! family and reports results.

use crate::fixtures;
use engram_domain::{CapabilityReason, CapabilityState};
use engram_runtime::CoreResult;

/// Status of a fixture execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureStatus {
    /// Fixture passed successfully.
    Passed,
    /// Fixture failed.
    Failed,
    /// Fixture was skipped (e.g., feature not enabled).
    Skipped,
}

/// Result of executing a single fixture.
#[derive(Debug, Clone)]
pub struct FixtureResult {
    /// Name of the fixture.
    pub name: String,

    /// Status of the fixture execution.
    pub status: FixtureStatus,

    /// Human-readable message explaining the result.
    pub message: String,

    /// Duration of fixture execution in milliseconds.
    pub duration_ms: u64,
}

/// Result of running conformance tests.
#[derive(Debug, Clone)]
pub struct ConformanceResult {
    /// Results for each capability family.
    pub fixtures: Vec<FixtureResult>,

    /// Overall duration in milliseconds.
    pub total_duration_ms: u64,
}

impl ConformanceResult {
    /// Converts fixture results to capability states.
    ///
    /// This maps fixture execution results to CapabilityState values that
    /// can be used in CapabilityReport.
    pub fn to_capability_states(&self) -> Vec<(String, CapabilityState)> {
        self.fixtures
            .iter()
            .map(|fixture| {
                let state = match fixture.status {
                    FixtureStatus::Passed => CapabilityState::Supported,
                    FixtureStatus::Failed => CapabilityState::Unsupported {
                        reason: CapabilityReason::ConformanceFailed,
                    },
                    FixtureStatus::Skipped => CapabilityState::Unsupported {
                        reason: CapabilityReason::FeatureDisabled,
                    },
                };
                (fixture.name.clone(), state)
            })
            .collect()
    }

    /// Returns the status for a specific capability family.
    ///
    /// Returns `None` if the fixture for that family was not run.
    pub fn get_status(&self, family: &str) -> Option<CapabilityState> {
        self.fixtures
            .iter()
            .find(|f| f.name == family)
            .map(|f| match f.status {
                FixtureStatus::Passed => CapabilityState::Supported,
                FixtureStatus::Failed => CapabilityState::Unsupported {
                    reason: CapabilityReason::ConformanceFailed,
                },
                FixtureStatus::Skipped => CapabilityState::Unsupported {
                    reason: CapabilityReason::FeatureDisabled,
                },
            })
    }
}

/// Conformance harness for running capability fixtures.
///
/// The harness executes fixtures for each capability family and returns
/// a structured report that can be used to populate CapabilityReport.
pub struct ConformanceHarness;

impl ConformanceHarness {
    /// Creates a new conformance harness.
    pub fn new() -> Self {
        Self
    }

    /// Runs all conformance fixtures.
    ///
    /// This method executes fixtures for each capability family and returns
    /// a structured report with results.
    ///
    /// # Errors
    ///
    /// Returns `CoreError::Adapter` if fixture execution fails.
    pub fn run_all(&self) -> CoreResult<ConformanceResult> {
        let start = std::time::Instant::now();

        // Run fixtures for each capability family
        let fixtures = vec![
            self.run_memory_fixture()?,
            self.run_knowledge_fixture()?,
            self.run_graph_fixture()?,
            self.run_ontology_fixture()?,
            self.run_taxonomy_fixture()?,
            self.run_beliefs_fixture()?,
            self.run_hierarchy_fixture()?,
            self.run_retrieval_fixture()?,
            self.run_vectors_fixture()?,
            self.run_migration_fixture()?,
        ];

        let total_duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConformanceResult {
            fixtures,
            total_duration_ms,
        })
    }

    /// Runs the memory capability fixture.
    fn run_memory_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::memory::run_memory_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "memory".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the knowledge capability fixture.
    fn run_knowledge_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::knowledge::run_knowledge_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "knowledge".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the graph capability fixture.
    fn run_graph_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::knowledge::run_graph_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "graph".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the ontology capability fixture.
    fn run_ontology_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::knowledge::run_ontology_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "ontology".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the taxonomy capability fixture.
    fn run_taxonomy_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::knowledge::run_taxonomy_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "taxonomy".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the beliefs capability fixture.
    fn run_beliefs_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::belief::run_belief_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "beliefs".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the hierarchy capability fixture.
    fn run_hierarchy_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::hierarchy::run_hierarchy_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "hierarchy".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the retrieval capability fixture.
    fn run_retrieval_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::retrieval::run_retrieval_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "retrieval".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the vectors capability fixture.
    fn run_vectors_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::vector::run_vector_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "vectors".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }

    /// Runs the migration capability fixture.
    fn run_migration_fixture(&self) -> CoreResult<FixtureResult> {
        let start = std::time::Instant::now();
        let result = fixtures::migration::run_migration_fixture();
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(FixtureResult {
            name: "migration".to_string(),
            status: if result.is_ok() {
                FixtureStatus::Passed
            } else {
                FixtureStatus::Failed
            },
            message: match result {
                Ok(_) => "fixture passed".to_string(),
                Err(e) => e.to_string(),
            },
            duration_ms,
        })
    }
}

impl Default for ConformanceHarness {
    fn default() -> Self {
        Self::new()
    }
}
