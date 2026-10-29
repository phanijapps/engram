use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, CodeAttempt, CodeBinding, CodeGeneration, RepoId, Scope};
use engram_knowledge::{CodeRefObservation, RefObservationPolicy};
use engram_runtime::{CoreError, CoreResult};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBindingStatus {
    pub binding: CodeBinding,
    pub attempt: Option<(CodeGeneration, CodeAttempt)>,
    pub code_epoch: u64,
    /// The full OID of the ACTIVE generation's pinned commit, read from the
    /// durable generation record (TASK-018). `None` when no generation is
    /// published or the cell cannot read generation records — never guessed
    /// from a checkout HEAD or scan-root checkpoint.
    pub indexed_commit_oid: Option<String>,
    /// On-request ref freshness for the CONFIGURED branch ref (TASK-018):
    /// the durable last observation reported SEPARATELY from
    /// `indexed_commit_oid`. Freshness is UNKNOWN when no valid observation
    /// exists — it is never invented from HEAD or an unrelated checkpoint.
    pub freshness: CodeRefFreshness,
    /// The binding-keyed checkpoint pointer for this repository (TASK-018):
    /// keyed by scope + RepoId + binding version + generation + OID +
    /// parser/filter fingerprints, never by a root basename or source
    /// string.
    pub checkpoint: CodeBindingCheckpoint,
}

/// Binding-keyed freshness facts derived from durable records only.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRefFreshness {
    /// The CONFIGURED branch ref (`refs/heads/…`) — the only ref this
    /// binding ever observes or compares against.
    pub branch_ref: String,
    /// The observation policy in force: on-request only.
    pub policy: RefObservationPolicy,
    /// The last successful observation's full OID, when one exists.
    pub observed_oid: Option<String>,
    /// The last successful observation's time, when one exists.
    pub observed_at: Option<engram_domain::Timestamp>,
    pub state: CodeFreshnessState,
}

/// Truthful freshness classification (plan §Contracts, Freshness row):
/// reported states never compare against an arbitrary checkout HEAD or the
/// freshest unrelated checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CodeFreshnessState {
    /// No valid observation exists. Freshness relative to the ref is
    /// UNKNOWN; a refresh (explicit observation) is required.
    Unknown,
    /// A durable observation exists. `matches_indexed` compares the
    /// OBSERVED ref OID with the indexed generation OID (`None` when the
    /// indexed OID itself is unreadable) — reported, never guessed.
    Observed { matches_indexed: Option<bool> },
}

/// The binding-keyed checkpoint: the stable pointer to the published
/// generation (plan §Contracts, Manifest/checkpoint row), keyed by scope,
/// RepoId, binding version, generation, commit OID, and parser/filter
/// fingerprints. Root basenames and caller-supplied manifest paths are
/// never part of the key.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBindingCheckpoint {
    pub repo_id: RepoId,
    pub branch_ref: String,
    pub binding_version: u64,
    pub code_epoch: u64,
    pub generation: Option<CheckpointGeneration>,
}

/// The immutable generation facts a checkpoint points at.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointGeneration {
    pub id: String,
    pub commit_oid: String,
    pub snapshot_digest: String,
    pub parser_fingerprint: String,
    pub filter_fingerprint: String,
}

impl CodeBindingCoordinator {
    pub async fn status(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        attempt_id: Option<&str>,
    ) -> CoreResult<CodeBindingStatus> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Inspect)
            .await?;
        for _ in 0..2 {
            let before = self.store.eligibility(scope).await?;
            let binding = self.store.get_binding(scope, repo).await?.ok_or_else(|| {
                CoreError::InvalidRequest {
                    reason: "BindingRequired".into(),
                }
            })?;
            let attempt = match attempt_id {
                Some(id) => self.store.get_attempt(scope, repo, id).await?,
                None => None,
            };
            // TASK-018: the indexed OID and the ref observation are read in
            // the SAME epoch-consistent window as the binding itself. One
            // durable generation read serves both the indexed OID and the
            // checkpoint pointer.
            let generation = self.active_generation_record(scope, repo, &binding).await?;
            let indexed_commit_oid = generation
                .as_ref()
                .map(|generation| generation.commit_oid.clone());
            let observation = self.store.get_ref_observation(scope, repo).await?;
            let freshness = CodeRefFreshness {
                branch_ref: binding.branch_ref.clone(),
                policy: RefObservationPolicy::OnRequest,
                observed_oid: observation
                    .as_ref()
                    .map(|observation| observation.observed_oid.clone()),
                observed_at: observation
                    .as_ref()
                    .map(|observation| observation.observed_at),
                state: match &observation {
                    None => CodeFreshnessState::Unknown,
                    Some(observation) => CodeFreshnessState::Observed {
                        matches_indexed: indexed_commit_oid
                            .as_deref()
                            .map(|indexed| indexed == observation.observed_oid),
                    },
                },
            };
            let checkpoint = CodeBindingCheckpoint {
                repo_id: repo.clone(),
                branch_ref: binding.branch_ref.clone(),
                binding_version: binding.version,
                code_epoch: before.epoch,
                generation: generation.map(|generation| CheckpointGeneration {
                    id: generation.id,
                    commit_oid: generation.commit_oid,
                    snapshot_digest: generation.snapshot_digest,
                    parser_fingerprint: generation.parser_fingerprint,
                    filter_fingerprint: generation.filter_fingerprint,
                }),
            };
            if self.store.eligibility(scope).await? == before {
                return Ok(CodeBindingStatus {
                    binding,
                    attempt,
                    code_epoch: before.epoch,
                    indexed_commit_oid,
                    freshness,
                    checkpoint,
                });
            }
        }
        Err(CoreError::Conflict {
            reason: "GenerationChanged".into(),
        })
    }

    /// Records one on-request ref observation (TASK-018): the caller
    /// supplies the full OID resolved from the CONFIGURED branch ref by the
    /// acquisition seam; the coordinator validates it, stamps the durable
    /// clock, and persists it SEPARATELY from the indexed generation OID.
    /// An observation naming any ref other than the bound branch is a typed
    /// `BindingConflict` denial. HEAD and scan-root checkpoints are never
    /// consulted.
    pub async fn record_ref_observation(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        observed_oid: &str,
    ) -> CoreResult<CodeRefObservation> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Inspect)
            .await?;
        let binding = self.store.get_binding(scope, repo).await?.ok_or_else(|| {
            CoreError::InvalidRequest {
                reason: "BindingRequired".into(),
            }
        })?;
        let observation = CodeRefObservation {
            branch_ref: binding.branch_ref.clone(),
            observed_oid: observed_oid.to_owned(),
            observed_at: self.clock.now(),
            policy: RefObservationPolicy::OnRequest,
        };
        observation
            .validate()
            .map_err(|_| CoreError::InvalidRequest {
                reason: "invalid ref observation".into(),
            })?;
        self.store
            .put_ref_observation(scope, repo, observation)
            .await
    }

    /// Reads the ACTIVE generation record bound to this (scope, repository,
    /// generation). A cell without generation reads reports `None`
    /// (unknown, never guessed); other failures propagate.
    async fn active_generation_record(
        &self,
        scope: &Scope,
        repo: &RepoId,
        binding: &CodeBinding,
    ) -> CoreResult<Option<CodeGeneration>> {
        let Some(generation_id) = binding.active_generation.as_deref() else {
            return Ok(None);
        };
        match self.store.get_generation(scope, repo, generation_id).await {
            Ok(generation) => Ok(generation),
            Err(CoreError::CapabilityUnsupported { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
