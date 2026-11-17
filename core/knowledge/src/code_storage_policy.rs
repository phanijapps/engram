//! Canonical portable reset-preview and code eligibility decisions.
use crate::{CodeBindingRepository, CodeEligibility, ConstrainedCodeQuery};
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};
fn storage_error() -> CoreError {
    CoreError::Adapter {
        adapter: "engram-code-binding".into(),
        message: "durable code authority is unavailable".into(),
    }
}
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn code_error(error: CodeBindingError) -> CoreError {
    match error {
        CodeBindingError::BindingConflict => CoreError::Conflict {
            reason: "BindingConflict".into(),
        },
        CodeBindingError::BindingRequired => CoreError::InvalidRequest {
            reason: "BindingRequired".into(),
        },
        CodeBindingError::StaleGeneration => CoreError::Conflict {
            reason: "GenerationChanged".into(),
        },
        _ => CoreError::InvalidRequest {
            reason: "invalid code binding state".into(),
        },
    }
}
pub async fn preview_reset<R: CodeBindingRepository + ?Sized>(
    repository: &R,
    scope: &Scope,
    repo_id: &RepoId,
    store_id: &str,
) -> CoreResult<CodeResetPreview> {
    let binding = repository
        .get_binding(scope, repo_id)
        .await?
        .ok_or_else(|| code_error(CodeBindingError::BindingRequired))?;
    let mut targets = Vec::new();
    let mut ambiguity = Vec::new();
    for entry in repository.target_lineage(scope).await? {
        if entry.repo_id != repo_id.0 || entry.retired {
            continue;
        }
        let Some(generation) = repository
            .get_generation(scope, repo_id, &entry.generation_id)
            .await?
        else {
            ambiguity.push(AmbiguousOwnership {
                record_id: entry.record_id,
                reason: "missing durable generation association".into(),
            });
            continue;
        };
        if entry.source_path.is_empty() {
            ambiguity.push(AmbiguousOwnership {
                record_id: entry.record_id,
                reason: "record-level code ownership cannot be proven".into(),
            });
            continue;
        }
        let lineage = CodeLineage {
            repo_id: repo_id.clone(),
            generation_id: entry.generation_id,
            source_path: entry.source_path,
            contributors: entry.contributors,
            span: entry.span,
        };
        let material = serde_json::to_vec(&(
            entry.record_id.clone(),
            &lineage,
            &generation.snapshot_digest,
            binding.version,
            binding.epoch,
        ))
        .map_err(|_| storage_error())?;
        let fingerprint = sha256_hex(&material);
        targets.push(CodeResetTarget {
            record_id: entry.record_id,
            store_id: store_id.into(),
            fingerprint,
            lineage,
            snapshot_ref: generation.snapshot_digest,
            scan_metadata_keys: vec!["bindingVersion".into(), "codeEpoch".into()],
            binding_version: binding.version,
            code_epoch: binding.epoch,
        });
    }
    if !ambiguity.is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: format!(
                "AmbiguousOwnership: {}",
                ambiguity
                    .iter()
                    .map(|item| item.record_id.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        });
    }
    if targets.is_empty() {
        return Err(code_error(CodeBindingError::InvalidResetPreview));
    }
    targets.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    let target_ids = targets
        .iter()
        .map(|target| target.record_id.clone())
        .collect();
    // Preservation inventory belongs to the recipe coordinator, which owns the
    // knowledge-store read boundary; this binding cell owns targets only.
    let preservation_inventory = engram_domain::PreservationInventory::default();
    let preservation_digests = vec![preservation_inventory.digest.clone()];
    let mut preview = CodeResetPreview {
        binding: binding.clone(),
        preview_hash: String::new(),
        target_ids,
        targets,
        ambiguity,
        preservation_digests,
        preservation_inventory,
        binding_versions: vec![(repo_id.clone(), binding.version)],
        code_epoch: binding.epoch,
    };
    preview.preview_hash = preview.content_hash().map_err(code_error)?;
    Ok(preview)
}
pub fn query_budget(limit: usize) -> CoreResult<()> {
    if limit == 0 {
        return Err(CoreError::InvalidRequest {
            reason: "authorized code query requires a nonzero budget".into(),
        });
    }
    Ok(())
}
pub fn foreign_target(ids: &[String]) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "BindingRequired: code target ids are foreign or unpublished in this scope: {}",
            ids.join(", ")
        ),
    }
}
pub fn constrain_query(
    eligibility: CodeEligibility,
    requested: &[String],
    eligible: &[String],
    limit: usize,
) -> CoreResult<ConstrainedCodeQuery> {
    query_budget(limit)?;
    if eligibility.generations.is_empty() {
        if requested.is_empty() {
            return Ok(ConstrainedCodeQuery {
                eligibility,
                target_ids: Vec::new(),
                limit,
            });
        }
        return Err(foreign_target(requested));
    }
    let target_ids = if requested.is_empty() {
        eligible.iter().take(limit).cloned().collect()
    } else {
        let mut result = Vec::with_capacity(requested.len());
        for id in requested {
            if !eligible.contains(id) {
                return Err(foreign_target(std::slice::from_ref(id)));
            }
            result.push(id.clone());
        }
        result
    };
    Ok(ConstrainedCodeQuery {
        eligibility,
        target_ids,
        limit,
    })
}
pub fn explicit_maintenance(requested: &[String]) -> CoreResult<()> {
    if requested.is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "maintenance authorization requires explicit target ids".into(),
        });
    }
    Ok(())
}

pub async fn prepare_reset_journal<R: CodeBindingRepository + ?Sized>(
    repository: &R,
    mut journal: CodeResetJournal,
) -> CoreResult<CodeResetJournal> {
    if journal.has_reserved_progress() {
        return Err(CoreError::InvalidRequest {
            reason: "ResetProgressSupplied: journal creation accepts no store-owned progress"
                .into(),
        });
    }
    journal.checkpoints.clear();
    journal.validate().map_err(code_error)?;
    let current = repository
        .preview_reset(
            &journal.preview.binding.scope,
            &journal.preview.binding.repo_id,
        )
        .await?;
    if current.targets != journal.preview.targets {
        return Err(CoreError::Conflict {
            reason: "ResetPreviewMismatch: preview or store state changed".into(),
        });
    }
    Ok(journal)
}
