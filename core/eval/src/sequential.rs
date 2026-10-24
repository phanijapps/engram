//! Matched sequential behavioral evaluation (MCCL-audit recommendation #3).
//!
//! Retrieval fixtures prove the write→retrieve loop; they cannot prove that
//! past experience improves FUTURE ACTIONS — the question continual-learning
//! memory must answer. This harness runs a scripted event stream against TWO
//! providers in lockstep: a **stateful** provider that accumulates everything,
//! and a **reset** provider that is rebuilt from scratch at every checkpoint
//! (only the writes before that point). An injected [`TaskProbe`] (the host
//! boundary — a model, a tool call, or a deterministic checker) scores each
//! probe against the context the provider returned.
//!
//! Reports the paper's core measurements, deterministically:
//!
//! - `ΔP[t] = P_stateful[t] − P_reset[t]` — the stateful-memory gain at each
//!   checkpoint, computed over IDENTICAL task inputs (matched protocol).
//! - `R[t, capability]` — the capability-by-update success matrix.
//! - **Interference ledger** — every capability that REGRESSES after a write
//!   (the paper's delayed-detection signal), with the interfering write.
//!
//! Rust owns sequencing, aggregation, and reset isolation; the host owns
//! what "success" means. No LLM, model provider, or storage engine enters
//! this module — both arms are `Arc<dyn MemoryService>`.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{ContextPayload, RetrievalRequest, WriteMemoryRequest};
use engram_memory::MemoryService;
use engram_runtime::CoreResult;
use serde::{Deserialize, Serialize};

/// One probe step: a query plus the capability it tests.
#[derive(Debug, Clone)]
pub struct ProbeStep {
    /// Retrieval request issued to BOTH arms (identical inputs — matched).
    pub retrieval: RetrievalRequest,
    /// Host-defined capability label (e.g. "payment-timeout-runbook").
    pub capability: String,
}

/// One scripted event.
#[derive(Debug, Clone)]
pub enum SequentialEvent {
    /// A memory write, replayed into both arms in order.
    Write(WriteMemoryRequest),
    /// A probe checkpoint: measure both arms BEFORE applying later writes.
    Probe(ProbeStep),
}

/// Host boundary: decides whether the retrieved context solves the task.
/// Implementations are models, tool runners, or deterministic checkers —
/// the harness only needs a boolean.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait TaskProbe: Send + Sync {
    /// Scores the retrieved context for the probe's capability.
    async fn check(&self, capability: &str, context: &ContextPayload) -> CoreResult<bool>;
}

/// Builds a FRESH provider for the reset arm (identical configuration).
#[allow(clippy::double_must_use)]
pub trait ResetProviderFactory: Send + Sync {
    fn fresh(&self) -> CoreResult<Arc<dyn MemoryService>>;
}

/// Factory adapter for cloneable providers (in-memory SQLite services).
pub struct CloneResetFactory {
    pub make: fn() -> CoreResult<Arc<dyn MemoryService>>,
}

impl ResetProviderFactory for CloneResetFactory {
    fn fresh(&self) -> CoreResult<Arc<dyn MemoryService>> {
        (self.make)()
    }
}

/// One interference-ledger row: a capability regressed after a write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterferenceEntry {
    /// The capability that previously passed and now fails.
    pub capability: String,
    /// Index of the interfering write event in the stream.
    pub after_write_index: usize,
    /// Checkpoint where the regression was observed.
    pub detected_at_checkpoint: usize,
}

/// One checkpoint's matched measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequentialCheckpoint {
    pub checkpoint: usize,
    /// Stateful arm success per capability probed here.
    pub stateful: Vec<(String, bool)>,
    /// Reset arm success per capability probed here.
    pub reset: Vec<(String, bool)>,
    /// `ΔP[t]` = stateful pass-rate − reset pass-rate at this checkpoint.
    pub gain: f32,
}

/// The full report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequentialReport {
    pub checkpoints: Vec<SequentialCheckpoint>,
    /// Capability × checkpoint success matrix for the STATEFUL arm.
    pub capability_matrix: Vec<(String, Vec<(usize, bool)>)>,
    /// Capabilities that regressed after a write (paper §4.2 interference).
    pub interference: Vec<InterferenceEntry>,
    /// Mean ΔP over all checkpoints (positive = stateful memory helps).
    pub mean_gain: f32,
}

/// Configuration for one harness run.
pub struct SequentialHarness {
    probe: Arc<dyn TaskProbe>,
    /// Index → write request, used for reset-arm replay and interference
    /// attribution.
    events: Vec<SequentialEvent>,
}

impl SequentialHarness {
    pub fn new(probe: Arc<dyn TaskProbe>, events: Vec<SequentialEvent>) -> Self {
        Self { probe, events }
    }

    /// Runs the matched protocol: both arms see identical events in the same
    /// order; the reset arm is rebuilt from scratch at every probe checkpoint
    /// with only the writes issued so far.
    pub async fn run(
        &self,
        stateful: Arc<dyn MemoryService>,
        reset_factory: Arc<dyn ResetProviderFactory>,
    ) -> CoreResult<SequentialReport> {
        let mut checkpoints = Vec::new();
        let mut matrix: Vec<(String, Vec<(usize, bool)>)> = Vec::new();
        let mut interference = Vec::new();
        // Last-known stateful success per capability (for regression detection).
        let mut last_success: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();
        // Writes applied to the stateful arm so far, in order — the reset arm
        // replays exactly this list at each probe (matched protocol).
        let mut applied_writes: Vec<&WriteMemoryRequest> = Vec::new();
        let mut last_write_index = 0usize;
        let mut checkpoint_index = 0usize;
        let mut gains = Vec::new();

        for event in &self.events {
            match event {
                SequentialEvent::Write(write) => {
                    stateful.write_memory(write.clone()).await?;
                    applied_writes.push(write);
                    last_write_index += 1;
                }
                SequentialEvent::Probe(step) => {
                    // Stateful arm.
                    let stateful_ctx = stateful.retrieve(step.retrieval.clone()).await?;
                    let stateful_ok = self.probe.check(&step.capability, &stateful_ctx).await?;

                    // Reset arm: fresh provider, replay ALL writes so far.
                    let reset = reset_factory.fresh()?;
                    for prior in &applied_writes {
                        reset.write_memory((*prior).clone()).await?;
                    }
                    let reset_ctx = reset.retrieve(step.retrieval.clone()).await?;
                    let reset_ok = self.probe.check(&step.capability, &reset_ctx).await?;

                    // Interference: a capability that passed before and fails
                    // now (on the stateful arm) regressed after a write.
                    if let Some(prev) = last_success.get(&step.capability) {
                        if *prev && !stateful_ok {
                            interference.push(InterferenceEntry {
                                capability: step.capability.clone(),
                                after_write_index: last_write_index,
                                detected_at_checkpoint: checkpoint_index,
                            });
                        }
                    }
                    last_success.insert(step.capability.clone(), stateful_ok);

                    match matrix.iter_mut().find(|(cap, _)| cap == &step.capability) {
                        Some((_, entries)) => entries.push((checkpoint_index, stateful_ok)),
                        None => matrix.push((
                            step.capability.clone(),
                            vec![(checkpoint_index, stateful_ok)],
                        )),
                    }

                    checkpoints.push(SequentialCheckpoint {
                        checkpoint: checkpoint_index,
                        stateful: vec![(step.capability.clone(), stateful_ok)],
                        reset: vec![(step.capability.clone(), reset_ok)],
                        gain: f32::from(stateful_ok as u8) - f32::from(reset_ok as u8),
                    });
                    gains.push(f32::from(stateful_ok as u8) - f32::from(reset_ok as u8));
                    checkpoint_index += 1;
                }
            }
        }

        let mean_gain = if gains.is_empty() {
            0.0
        } else {
            gains.iter().sum::<f32>() / gains.len() as f32
        };
        Ok(SequentialReport {
            checkpoints,
            capability_matrix: matrix,
            interference,
            mean_gain,
        })
    }
}
