//! Bound code ingestion through registered-locator acquisition + generation
//! staging/publication (review r6 finding 2).
//!
//! Code bytes NEVER enter the knowledge graph from a caller-supplied path.
//! The one production ingestion path is [`CodeBindingCoordinator::ingest_registered`]:
//! resolve the repository's REGISTERED acquisition locator, acquire the BOUND
//! branch (immutable committed bytes, never a mutable checkout), retain the
//! exact snapshot, prepare the generation's staged records, write them
//! durably (knowledge lanes through the neutral knowledge/graph handles,
//! lineage + source descriptors through the binding cell's own durable
//! ports), and publish through the SAME authorized
//! [`CodeBindingCoordinator::rebuild`] seam with typed receipts. The old
//! binding-existence-unlocks-a-checkout-scan shape is gone from every
//! surface that routes here.

use super::{CodeBindingCoordinator, CodeGenerationStages, CodeRebuildRequest};
use engram_domain::{Actor, CodeAttempt, CodeBinding, CodeGeneration, RepoId, Scope};
use engram_knowledge::{CodeBindingRepository, CoreError, CoreResult, StageReceipt};

use std::path::PathBuf;
use std::sync::Arc;

/// The neutral handles bound-code ingestion needs from its host (recipe):
/// knowledge/graph write lanes and the local custody root for retained
/// snapshots. Lineage/source staging and receipts go through the
/// coordinator's own durable binding cell — the same cell that serves
/// `target_lineage` to eligibility views, so what this pipeline stages is
/// exactly what the pinned generation view later serves.
pub struct CodeIngestionLanes {
    pub knowledge: Arc<dyn engram_knowledge::KnowledgeRepository>,
    pub graph: Arc<dyn engram_knowledge::KnowledgeGraphRepository>,
    /// Retained-snapshot custody root (the same root the recipe's
    /// `RetainedSnapshotReader` serves exact reads from).
    pub retained_root: PathBuf,
}

/// The outcome of one ingestion pass: the post-publish binding plus the
/// generation's facts and staged-record counts.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestedGeneration {
    pub repo_id: RepoId,
    pub branch_ref: String,
    pub commit_oid: String,
    pub generation_id: String,
    pub snapshot_digest: String,
    pub files: usize,
    pub documents: usize,
    pub entities: usize,
    pub relationships: usize,
    /// True when the generation was already the binding's active generation
    /// (idempotent re-ingest of the same pinned commit — no writes).
    pub idempotent: bool,
    pub binding: CodeBinding,
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}

/// Deterministic generation identity BEFORE retention: derived from the
/// pinned commit, the binding version, and the parser/filter fingerprints —
/// identical inputs always address the same generation (idempotent retries).
fn generation_id(commit_oid: &str, binding_version: u64, fingerprints: &str) -> String {
    let identity =
        engram_ingest::content_hash(format!("{commit_oid}:{binding_version}:{fingerprints}"));
    let identity = identity.trim_start_matches("sha256:");
    format!(
        "gen-{}-v{binding_version}",
        &identity[..24.min(identity.len())]
    )
}

/// The stages of one registered ingestion: the SOURCE lane receipts the
/// retained snapshot digest; the KNOWLEDGE lane performs the durable record
/// writes plus lineage/source staging through the binding cell and receipts
/// the manifest-derived digest (identical inputs ⇒ identical receipt, so
/// idempotent retries validate).
struct RegisteredIngestStages<'a> {
    binding: &'a CodeBinding,
    generation: &'a CodeGeneration,
    prepared: &'a engram_ingest::PreparedGeneration,
    snapshot_digest: &'a str,
    knowledge: Arc<dyn engram_knowledge::KnowledgeRepository>,
    graph: Arc<dyn engram_knowledge::KnowledgeGraphRepository>,
    store: Arc<dyn CodeBindingRepository>,
}

#[async_trait::async_trait]
impl CodeGenerationStages for RegisteredIngestStages<'_> {
    async fn stage(
        &self,
        lane: &engram_domain::CodeReceiptKind,
        _binding: &CodeBinding,
        _generation: &CodeGeneration,
        _attempt: &CodeAttempt,
    ) -> CoreResult<StageReceipt> {
        let lane_name = serde_json::to_value(lane)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| invalid("receipt lane could not be serialized"))?;
        match lane_name.as_str() {
            "source" => Ok(StageReceipt {
                attempt_id: self.generation.id.clone(),
                generation_id: self.generation.id.clone(),
                lane: lane_name,
                digest: self.snapshot_digest.to_owned(),
            }),
            "knowledge" => {
                let repo = engram_ingest::KnowledgeRepoGraph::new(
                    Arc::clone(&self.knowledge),
                    Arc::clone(&self.graph),
                );
                engram_ingest::write_prepared_generation(&repo, self.prepared).await?;
                for (record_id, lineage) in &self.prepared.lineage {
                    self.store
                        .put_lineage(&self.binding.scope, record_id, lineage.clone())
                        .await?;
                }
                for document in &self.prepared.documents {
                    self.store
                        .put_source_descriptor(&self.binding.scope, document.source.clone())
                        .await?;
                }
                // Manifest-derived receipt digest over the binding-keyed
                // manifest identity + staged lineage + source descriptors —
                // the same tuple shape `engram_ingest`'s manifest receipts
                // hash. Receipt custody is the binding cell's durable
                // receipts table (recorded by `rebuild` itself).
                let sources: Vec<&engram_domain::IndexedSource> = self
                    .prepared
                    .documents
                    .iter()
                    .map(|document| &document.source)
                    .collect();
                let bytes = serde_json::to_vec(&(
                    engram_ingest::generation_manifest_key(
                        self.binding,
                        self.prepared,
                        &self.generation.id,
                    )?,
                    &self.prepared.lineage,
                    sources,
                ))
                .map_err(|_| invalid("knowledge receipt could not be serialized"))?;
                let digest = engram_ingest::content_hash(bytes)
                    .trim_start_matches("sha256:")
                    .to_owned();
                Ok(StageReceipt {
                    attempt_id: self.generation.id.clone(),
                    generation_id: self.generation.id.clone(),
                    lane: lane_name,
                    digest,
                })
            }
            other => Err(CoreError::CapabilityUnsupported {
                capability: "code-ingestion-lane".into(),
                reason: format!("registered ingestion does not stage lane {other:?}"),
            }),
        }
    }
}

impl CodeBindingCoordinator {
    /// ONE bound-code ingestion pass, acquired from the REGISTERED locator
    /// (review r6 finding 2). The caller names the repository; the locator,
    /// branch, bytes, and generation identity all come from durable
    /// authority. `branch_assertion` is optional equality-only. Re-ingesting
    /// the same pinned commit is an idempotent no-op; a different commit
    /// publishes a new generation of the same binding.
    pub async fn ingest_registered(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        branch_assertion: Option<&str>,
        filter_config: Option<&engram_ingest::ScanFilterConfig>,
        lanes: &CodeIngestionLanes,
    ) -> CoreResult<IngestedGeneration> {
        let registration = self.store.get_registration(repo).await?.ok_or_else(|| {
            invalid("BindingRequired: no registered repository — register a locator first")
        })?;
        match registration.acquisition {
            engram_knowledge::AcquisitionMode::LocalOnly => {}
            engram_knowledge::AcquisitionMode::ConfiguredRemote => {
                return Err(CoreError::CapabilityUnsupported {
                    capability: "code-remote-ingest".into(),
                    reason: "configured-remote acquisition is not wired on this transport; \
                             remote ingestion requires the remote acquisition path"
                        .into(),
                });
            }
        }
        let binding =
            self.store.get_binding(scope, repo).await?.ok_or_else(|| {
                invalid("BindingRequired: bind the repository before ingesting code")
            })?;
        if let Some(asserted) = branch_assertion
            && asserted != binding.branch_ref
        {
            return Err(CoreError::Conflict {
                reason: "BindingConflict: asserted branch does not match the bound ref".into(),
            });
        }
        // Acquire the BOUND branch from the REGISTERED locator — committed
        // bytes only; the caller never supplies a path or a checkout.
        let (commit, files) = engram_ingest::git_source::acquire_local_branch(
            std::path::Path::new(&registration.locator_id),
            &binding.branch_ref,
        )?;
        let scan_filter = match filter_config {
            Some(config) => engram_ingest::ScanFilter::merge(config),
            None => engram_ingest::ScanFilter::builtin(),
        };
        let filter_fingerprint = match filter_config {
            Some(config) => {
                let json = serde_json::to_string(config)
                    .map_err(|_| invalid("scan filter could not be serialized"))?;
                engram_ingest::content_hash(json)
                    .trim_start_matches("sha256:")
                    .to_owned()
            }
            None => "builtin-v1".to_owned(),
        };
        let fingerprints = format!("code-symbol-v1:{filter_fingerprint}");
        let generation = CodeGeneration {
            id: generation_id(&commit.oid, binding.version, &fingerprints),
            binding_version: binding.version,
            commit_oid: commit.oid.clone(),
            snapshot_digest: String::new(),
            parser_fingerprint: "code-symbol-v1".into(),
            filter_fingerprint: filter_fingerprint.clone(),
        };
        // Idempotent short-circuit: the binding is already published at this
        // generation — the same pinned commit re-ingested. No writes at all.
        if binding.active_generation.as_deref() == Some(&generation.id) {
            let durable = self
                .store
                .get_generation(scope, repo, &generation.id)
                .await?
                .ok_or_else(|| CoreError::Conflict {
                    reason: "GenerationChanged: active generation has no durable record".into(),
                })?;
            return Ok(IngestedGeneration {
                repo_id: repo.clone(),
                branch_ref: binding.branch_ref.clone(),
                commit_oid: durable.commit_oid,
                generation_id: durable.id,
                snapshot_digest: durable.snapshot_digest,
                files: files.len(),
                documents: 0,
                entities: 0,
                relationships: 0,
                idempotent: true,
                binding,
            });
        }
        // Retain the exact snapshot (immutable committed bytes). A prior
        // retention for THIS commit is the same bytes (content-addressed by
        // oid): recover the digest from the durable generation record; a
        // snapshot without one is a crash-window repair state, denied typed.
        let snapshot_digest = match engram_ingest::git_source::retain_snapshot(
            &lanes.retained_root,
            commit.clone(),
            &files,
        ) {
            Ok(snapshot) => snapshot.digest,
            Err(CoreError::Conflict { reason })
                if reason.contains("snapshot for this commit already exists") =>
            {
                self.store
                    .get_generation(scope, repo, &generation.id)
                    .await?
                    .ok_or_else(|| CoreError::Conflict {
                        reason: "retained snapshot exists without a durable generation record \
                                 — repair required"
                            .into(),
                    })?
                    .snapshot_digest
            }
            Err(error) => return Err(error),
        };
        let generation = CodeGeneration {
            snapshot_digest: snapshot_digest.clone(),
            ..generation
        };
        generation
            .validate()
            .map_err(|_| invalid("invalid code generation for registered ingestion"))?;
        let options = engram_ingest::ScanOptions {
            scope: scope.clone(),
            policy: ingest_policy(),
            actor: actor.clone(),
            source_name: format!("code-ingest:{}", generation.id),
            max_bytes: 0,
            manifest: std::collections::HashMap::new(),
            scan_filter,
        };
        let prepared = engram_ingest::prepare_generation(
            &binding,
            &generation,
            &engram_ingest::CommittedGenerationFiles {
                commit: &commit,
                files: &files,
            },
            &options,
        )?;
        let documents = prepared.documents.len();
        let entities = prepared
            .documents
            .iter()
            .map(|document| document.graph.entities.len())
            .sum::<usize>();
        let relationships = prepared
            .documents
            .iter()
            .map(|document| document.graph.relationships.len())
            .sum::<usize>();
        let stages = RegisteredIngestStages {
            binding: &binding,
            generation: &generation,
            prepared: &prepared,
            snapshot_digest: &snapshot_digest,
            knowledge: Arc::clone(&lanes.knowledge),
            graph: Arc::clone(&lanes.graph),
            store: self.store(),
        };
        let published = self
            .rebuild(
                actor,
                CodeRebuildRequest {
                    scope: scope.clone(),
                    repo_id: repo.clone(),
                    generation: generation.clone(),
                    attempt_id: generation.id.clone(),
                    idempotency_key: format!("ingest-{}-{}", repo.0, generation.id),
                },
                &stages,
            )
            .await?;
        Ok(IngestedGeneration {
            repo_id: repo.clone(),
            branch_ref: binding.branch_ref.clone(),
            commit_oid: generation.commit_oid.clone(),
            generation_id: generation.id.clone(),
            snapshot_digest,
            files: files.len(),
            documents,
            entities,
            relationships,
            idempotent: false,
            binding: published,
        })
    }
}

/// Default write policy for staged code records (workspace-visible,
/// durable — the same policy every scan lane uses).
fn ingest_policy() -> engram_domain::Policy {
    engram_domain::Policy {
        visibility: engram_domain::Visibility::Workspace,
        retention: engram_domain::Retention::Durable,
        sensitivity: Some(engram_domain::Sensitivity::Low),
        allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(engram_domain::DeleteMode::Tombstone),
    }
}
