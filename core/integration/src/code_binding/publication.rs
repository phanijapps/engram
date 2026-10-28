use super::{CodeAuthorityAction, CodeBindingCoordinator, CodeGenerationStages};
use engram_domain::{
    Actor, CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, RepoId, Scope,
};
use engram_runtime::{CoreError, CoreResult};

pub struct CodeRebuildRequest {
    pub scope: Scope,
    pub repo_id: RepoId,
    pub generation: CodeGeneration,
    pub attempt_id: String,
    pub idempotency_key: String,
}
fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }
}
fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "invalid code publication attempt or receipt".into(),
    }
}
impl CodeBindingCoordinator {
    pub async fn rebuild(
        &self,
        actor: &Actor,
        request: CodeRebuildRequest,
        stages: &dyn CodeGenerationStages,
    ) -> CoreResult<CodeBinding> {
        self.authorizer
            .authorize(
                actor,
                &request.scope,
                &request.repo_id,
                CodeAuthorityAction::Rebuild,
            )
            .await?;
        request.generation.validate().map_err(|_| invalid())?;
        if request.attempt_id.is_empty() || request.idempotency_key.is_empty() {
            return Err(invalid());
        }
        let binding = self
            .store
            .get_binding(&request.scope, &request.repo_id)
            .await?
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "BindingRequired".into(),
            })?;
        if binding.version != request.generation.binding_version {
            return Err(changed());
        }
        let previous = self
            .store
            .get_attempt(&request.scope, &request.repo_id, &request.attempt_id)
            .await?;
        let attempt = if let Some((generation, attempt)) = previous {
            if generation != request.generation
                || attempt.idempotency_key != request.idempotency_key
                || attempt.receipt_kinds != self.required_lanes
                || attempt.binding_version != binding.version
            {
                return Err(invalid());
            }
            if attempt.state == CodeAttemptState::Published {
                if binding.active_generation.as_deref() != Some(&generation.id)
                    || binding.epoch != attempt.expected_epoch + 1
                {
                    return Err(changed());
                }
                return Ok(binding);
            }
            if binding.epoch != attempt.expected_epoch
                || binding.active_generation != attempt.expected_active_generation
                || !matches!(
                    attempt.state,
                    CodeAttemptState::Created
                        | CodeAttemptState::Acquiring
                        | CodeAttemptState::Staging
                        | CodeAttemptState::Ready
                )
            {
                return Err(changed());
            }
            attempt
        } else {
            self.store
                .begin_attempt(
                    &request.scope,
                    &request.repo_id,
                    request.generation.clone(),
                    CodeAttempt {
                        id: request.attempt_id.clone(),
                        idempotency_key: request.idempotency_key,
                        binding_version: binding.version,
                        expected_epoch: binding.epoch,
                        expected_active_generation: binding.active_generation.clone(),
                        fencing_token: 0,
                        state: CodeAttemptState::Staging,
                        receipt_kinds: self.required_lanes.clone(),
                    },
                )
                .await?
        };
        for lane in &self.required_lanes {
            let receipt = stages
                .stage(lane, &binding, &request.generation, &attempt)
                .await?;
            let lane_name = serde_json::to_value(lane).map_err(|_| invalid())?;
            if receipt.attempt_id != attempt.id
                || receipt.generation_id != request.generation.id
                || Some(receipt.lane.as_str()) != lane_name.as_str()
                || receipt.digest.len() != 64
                || !receipt.digest.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid());
            }
            self.store.record_receipt(receipt).await?;
        }
        self.store
            .mark_ready(
                &request.scope,
                &request.repo_id,
                &attempt.id,
                attempt.fencing_token,
            )
            .await?;
        self.store
            .publish(
                &request.scope,
                &request.repo_id,
                &attempt.id,
                attempt.expected_epoch,
            )
            .await
    }
}
