//! Durable scoped code authority storage. Host authorization remains canonical.
use crate::{HelixStore, records::*, transaction::Fence};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::*;
use helix_db::dsl::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
pub(crate) fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }
}
pub(crate) fn invalid() -> CoreError {
    CoreError::InvalidRequest {
        reason: "invalid code binding state".into(),
    }
}
pub(crate) fn tuple(values: &[&str]) -> String {
    serde_json::to_string(values).expect("string tuple serializes")
}
pub(crate) fn increment(value: u64) -> CoreResult<u64> {
    value.checked_add(1).ok_or_else(changed)
}
fn code_body_value<T: DeserializeOwned>(value: Value) -> crate::error::Result<T> {
    serde_json::from_value(value).map_err(|_| crate::error::Error::IncompatibleSchema)
}
fn code_tuple(id: &str, length: usize) -> crate::error::Result<Vec<String>> {
    let values: Vec<String> =
        serde_json::from_str(id).map_err(|_| crate::error::Error::IncompatibleSchema)?;
    if values.len() != length || tuple(&values.iter().map(String::as_str).collect::<Vec<_>>()) != id
    {
        return Err(crate::error::Error::IncompatibleSchema);
    }
    Ok(values)
}
impl HelixStore {
    /// Validate the private envelope and every scalar used by native code fences.
    /// Parent checks are separate, one-way lookups; decoding never recursively validates parents.
    pub(crate) fn validate_code_payload(
        &self,
        family: &str,
        payload: &Value,
        physical: Option<&Value>,
    ) -> crate::error::Result<()> {
        use crate::error::Error::IncompatibleSchema as Bad;
        if !matches!(
            family,
            CODE_REGISTRATION
                | CODE_BINDING
                | CODE_GENERATION
                | CODE_GENERATION_OWNER
                | CODE_ATTEMPT
                | CODE_LINEAGE
                | CODE_SOURCE
                | CODE_STAGE_RECEIPT
                | CODE_SCOPE
                | CODE_FENCE
                | CODE_LEASE
                | CODE_LEASE_COVERAGE
                | CODE_LEASE_TOMBSTONE
                | CODE_RESET
                | CODE_RESET_PROGRESS
                | CODE_OBSERVATION
        ) {
            return Ok(());
        }
        let id = payload["id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or(Bad)?;
        let scope: Scope = code_body_value(payload["scope"].clone())?;
        if scope != self.inner.partition.scope {
            return Err(Bad);
        }
        let body = &payload["record"];
        let field = |name: &str, expected: Value| -> crate::error::Result<()> {
            if payload.get(name) != Some(&expected) {
                return Err(Bad);
            }
            Ok(())
        };
        match family {
            CODE_REGISTRATION => {
                let r: RepositoryRegistration = code_body_value(body.clone())?;
                if r.repo_id.0 != id
                    || r.repo_id.0.trim().is_empty()
                    || r.locator_id.trim().is_empty()
                {
                    return Err(Bad);
                }
                field("repoId", json!(r.repo_id))?;
            }
            CODE_BINDING => {
                let r: CodeBinding = code_body_value(body.clone())?;
                r.validate().map_err(|_| Bad)?;
                if r.repo_id.0 != id || r.scope != scope {
                    return Err(Bad);
                }
                field("repoId", json!(r.repo_id))?;
                field("epoch_key", json!(r.epoch.to_string()))?;
            }
            CODE_GENERATION | CODE_GENERATION_OWNER => {
                let r: CodeGeneration = code_body_value(body.clone())?;
                r.validate().map_err(|_| Bad)?;
                if family == CODE_GENERATION {
                    if r.id != id {
                        return Err(Bad);
                    }
                } else {
                    let key = code_tuple(id, 2)?;
                    if key[1] != r.id || key[0].trim().is_empty() {
                        return Err(Bad);
                    }
                    field("repoId", json!(key[0]))?;
                }
                field("generationId", json!(r.id))?;
            }
            CODE_ATTEMPT => {
                let r: AttemptRow = code_body_value(body.clone())?;
                r.generation.validate().map_err(|_| Bad)?;
                r.attempt.validate().map_err(|_| Bad)?;
                if r.attempt.id != id
                    || r.repo.0.trim().is_empty()
                    || r.generation.binding_version != r.attempt.binding_version
                {
                    return Err(Bad);
                }
                field("repoId", json!(r.repo))?;
                field("generationId", json!(r.generation.id))?;
                field("fencing_key", json!(r.attempt.fencing_token.to_string()))?;
            }
            CODE_LINEAGE => {
                let r: TargetLineage = code_body_value(body.clone())?;
                CodeLineage {
                    repo_id: RepoId(r.repo_id.clone()),
                    generation_id: r.generation_id.clone(),
                    source_path: r.source_path.clone(),
                    span: r.span,
                    contributors: r.contributors.clone(),
                }
                .validate()
                .map_err(|_| Bad)?;
                if r.record_id != id {
                    return Err(Bad);
                }
                field("repoId", json!(r.repo_id))?;
                field("generationId", json!(r.generation_id))?;
                field("retired", json!(r.retired))?;
            }
            CODE_SOURCE => {
                let r: IndexedSource = code_body_value(body.clone())?;
                r.validate().map_err(|_| Bad)?;
                RetainedPath::new(&r.path).map_err(|_| Bad)?;
                if id != tuple(&[&r.repo_id.0, &r.generation_id, &r.path]) {
                    return Err(Bad);
                }
                field("repoId", json!(r.repo_id))?;
                field("generationId", json!(r.generation_id))?;
                field("path", json!(r.path))?;
            }
            CODE_STAGE_RECEIPT => {
                let r: StageReceipt = code_body_value(body.clone())?;
                if r.attempt_id.is_empty()
                    || r.generation_id.is_empty()
                    || r.digest.is_empty()
                    || r.lane.is_empty()
                    || id != tuple(&[&r.attempt_id, &r.lane])
                {
                    return Err(Bad);
                }
                field("attemptId", json!(r.attempt_id))?;
                field("generationId", json!(r.generation_id))?;
            }
            CODE_SCOPE | CODE_FENCE => {
                let _: u64 = code_body_value(body.clone())?;
                if id
                    != if family == CODE_SCOPE {
                        "scope"
                    } else {
                        "code-writer"
                    }
                {
                    return Err(Bad);
                }
            }
            CODE_LEASE => {
                let r: CodeReadLease = code_body_value(body.clone())?;
                if r.id != id
                    || r.fencing_token == 0
                    || r.eligibility.scope != scope
                    || r.eligibility
                        .generations
                        .iter()
                        .chain(&r.eligibility.repository_ids)
                        .any(|v| v.trim().is_empty())
                {
                    return Err(Bad);
                }
                field("fence_key", json!(r.fencing_token.to_string()))?;
                field("expires_ms", json!(r.expires_at.timestamp_millis()))?;
                field("expires_secs", json!(r.expires_at.timestamp()))?;
                field(
                    "expires_nanos",
                    json!(r.expires_at.timestamp_subsec_nanos()),
                )?;
            }
            CODE_LEASE_COVERAGE => {
                let lease: String = code_body_value(body.clone())?;
                let key = code_tuple(id, 2)?;
                if lease.is_empty() || key[0] != lease || key[1].is_empty() {
                    return Err(Bad);
                }
                field("leaseId", json!(lease))?;
                field("generationId", json!(key[1]))?;
                if payload["expires_secs"].as_i64().is_none()
                    || payload["expires_ms"].as_i64().is_none()
                    || payload["expires_nanos"]
                        .as_u64()
                        .is_none_or(|n| n >= 1_000_000_000)
                {
                    return Err(Bad);
                }
            }
            CODE_LEASE_TOMBSTONE => {
                if body.as_str() != Some(id) {
                    return Err(Bad);
                }
            }
            CODE_RESET => {
                let r: CodeResetJournal = code_body_value(body.clone())?;
                r.validate().map_err(|_| Bad)?;
                if r.reset_id != id || r.preview.binding.scope != scope {
                    return Err(Bad);
                }
                field("repoId", json!(r.preview.binding.repo_id))?;
            }
            CODE_RESET_PROGRESS => {
                let key = code_tuple(id, 2)?;
                if key.iter().any(|s| s.is_empty())
                    || body.as_str() != Some(format!("completed:{}", key[1]).as_str())
                {
                    return Err(Bad);
                }
                field("resetId", json!(key[0]))?;
                field("recordId", json!(key[1]))?;
            }
            CODE_OBSERVATION => {
                let r: CodeRefObservation = code_body_value(body.clone())?;
                r.validate().map_err(|_| Bad)?;
                field("repoId", json!(id))?;
            }
            _ => unreachable!(),
        }
        if let Some(row) = physical {
            for (name, v) in payload.as_object().ok_or(Bad)? {
                if !matches!(name.as_str(), "id" | "scope" | "record")
                    && (v.is_string() || v.is_boolean() || v.is_number())
                    && row.get(name) != Some(v)
                {
                    return Err(Bad);
                }
            }
        }
        Ok(())
    }
    fn code_parent<T: DeserializeOwned>(
        &self,
        family: &str,
        id: &str,
    ) -> crate::error::Result<Option<T>> {
        // get validates local identity/projections only: parent traversal cannot cycle.
        self.get::<Value>(family, id)?
            .map(|v| code_body_value(v["record"].clone()))
            .transpose()
    }
    fn validate_code_binding_publication(&self, r: &CodeBinding) -> crate::error::Result<()> {
        use crate::error::Error::IncompatibleSchema as Bad;
        let generation = |id: &str| self.code_parent::<CodeGeneration>(CODE_GENERATION, id);
        let owner = |repo: &str, g: &str| {
            self.code_parent::<CodeGeneration>(CODE_GENERATION_OWNER, &tuple(&[repo, g]))
        };
        if self
            .code_parent::<RepositoryRegistration>(CODE_REGISTRATION, &r.repo_id.0)?
            .is_none()
        {
            return Err(Bad);
        }
        if let Some(g) = &r.active_generation {
            let owned = owner(&r.repo_id.0, g)?.ok_or(Bad)?;
            if generation(g)?.as_ref() != Some(&owned) || owned.binding_version != r.version {
                return Err(Bad);
            }
            let mut published = false;
            for v in self.select_values(CODE_ATTEMPT, None)? {
                let a: AttemptRow = code_body_value(v["record"].clone())?;
                if a.repo == r.repo_id
                    && a.generation == owned
                    && a.attempt.state == CodeAttemptState::Published
                    && a.attempt.binding_version == r.version
                    && a.attempt
                        .expected_epoch
                        .checked_add(1)
                        .is_some_and(|e| e <= r.epoch)
                {
                    published = true;
                }
            }
            if !published {
                return Err(Bad);
            }
        }
        Ok(())
    }
    fn validate_code_lease_parents(&self, r: &CodeReadLease) -> crate::error::Result<()> {
        use crate::error::Error::IncompatibleSchema as Bad;
        let generation = |id: &str| self.code_parent::<CodeGeneration>(CODE_GENERATION, id);
        let owner = |repo: &str, g: &str| {
            self.code_parent::<CodeGeneration>(CODE_GENERATION_OWNER, &tuple(&[repo, g]))
        };
        for repo in &r.eligibility.repository_ids {
            if self
                .code_parent::<RepositoryRegistration>(CODE_REGISTRATION, repo)?
                .is_none()
            {
                return Err(Bad);
            }
        }
        let attempts = self
            .select_values(CODE_ATTEMPT, None)?
            .into_iter()
            .map(|v| code_body_value::<AttemptRow>(v["record"].clone()))
            .collect::<crate::error::Result<Vec<_>>>()?;
        // Flat sets remain flat: no zipped/cross-pair wire meaning is introduced.
        // Historical parents are immutable; current active pointers/epochs are irrelevant.
        for g in &r.eligibility.generations {
            let immutable = generation(g)?.ok_or(Bad)?;
            let mut published = false;
            for attempt in &attempts {
                if attempt.generation == immutable
                    && attempt.attempt.state == CodeAttemptState::Published
                    && r.eligibility.repository_ids.contains(&attempt.repo.0)
                    && owner(&attempt.repo.0, g)?.as_ref() == Some(&immutable)
                {
                    published = true;
                }
            }
            if !published {
                return Err(Bad);
            }
        }
        Ok(())
    }
    fn validate_code_parents(&self, family: &str, payload: &Value) -> crate::error::Result<()> {
        use crate::error::Error::IncompatibleSchema as Bad;
        let body = &payload["record"];
        let generation = |id: &str| self.code_parent::<CodeGeneration>(CODE_GENERATION, id);
        let owner = |repo: &str, g: &str| {
            self.code_parent::<CodeGeneration>(CODE_GENERATION_OWNER, &tuple(&[repo, g]))
        };
        match family {
            CODE_GENERATION_OWNER => {
                let r: CodeGeneration = code_body_value(body.clone())?;
                let key = code_tuple(payload["id"].as_str().ok_or(Bad)?, 2)?;
                if self
                    .code_parent::<RepositoryRegistration>(CODE_REGISTRATION, &key[0])?
                    .is_none()
                    || generation(&r.id)?.as_ref() != Some(&r)
                {
                    return Err(Bad);
                }
            }
            CODE_ATTEMPT => {
                let r: AttemptRow = code_body_value(body.clone())?;
                if self
                    .code_parent::<RepositoryRegistration>(CODE_REGISTRATION, &r.repo.0)?
                    .is_none()
                    || generation(&r.generation.id)?.as_ref() != Some(&r.generation)
                    || owner(&r.repo.0, &r.generation.id)?.as_ref() != Some(&r.generation)
                {
                    return Err(Bad);
                }
            }
            CODE_BINDING => {
                let r: CodeBinding = code_body_value(body.clone())?;
                self.validate_code_binding_publication(&r)?;
            }
            CODE_LINEAGE | CODE_SOURCE => {
                let (repo, g, commit) = if family == CODE_LINEAGE {
                    let r: TargetLineage = code_body_value(body.clone())?;
                    (r.repo_id, r.generation_id, None)
                } else {
                    let r: IndexedSource = code_body_value(body.clone())?;
                    (r.repo_id.0, r.generation_id, Some(r.commit_oid))
                };
                // Staging before an attempt is legitimate. Existing immutable parents must agree.
                let stored = generation(&g)?;
                if let Some(owned) = owner(&repo, &g)? {
                    if stored.as_ref() != Some(&owned) {
                        return Err(Bad);
                    }
                }
                if let (Some(stored), Some(commit)) = (stored, commit) {
                    if !crate::code_generation::source_matches_generation(&stored, &g, &commit) {
                        return Err(Bad);
                    }
                }
            }
            CODE_STAGE_RECEIPT => {
                let r: StageReceipt = code_body_value(body.clone())?;
                let parent: AttemptRow =
                    self.code_parent(CODE_ATTEMPT, &r.attempt_id)?.ok_or(Bad)?;
                if parent.generation.id != r.generation_id
                    || !parent
                        .attempt
                        .receipt_kinds
                        .iter()
                        .any(|k| serde_json::to_value(k).is_ok_and(|v| v.as_str() == Some(&r.lane)))
                {
                    return Err(Bad);
                }
                if generation(&parent.generation.id)?.as_ref() != Some(&parent.generation)
                    || owner(&parent.repo.0, &parent.generation.id)?.as_ref()
                        != Some(&parent.generation)
                {
                    return Err(Bad);
                }
            }
            CODE_LEASE_COVERAGE => {
                let lease = body.as_str().ok_or(Bad)?;
                let parent: CodeReadLease = self.code_parent(CODE_LEASE, lease)?.ok_or(Bad)?;
                self.validate_code_lease_parents(&parent)?;
                let g = payload["generationId"].as_str().ok_or(Bad)?;
                if !parent.eligibility.generations.iter().any(|v| v == g)
                    || payload["expires_secs"] != json!(parent.expires_at.timestamp())
                    || payload["expires_nanos"] != json!(parent.expires_at.timestamp_subsec_nanos())
                    || payload["expires_ms"] != json!(parent.expires_at.timestamp_millis())
                {
                    return Err(Bad);
                }
            }
            CODE_LEASE => {
                let r: CodeReadLease = code_body_value(body.clone())?;
                self.validate_code_lease_parents(&r)?;
            }
            CODE_RESET => {
                let r: CodeResetJournal = code_body_value(body.clone())?;
                self.validate_code_binding_publication(&r.preview.binding)?;
            }
            CODE_OBSERVATION => {
                if self
                    .code_parent::<RepositoryRegistration>(
                        CODE_REGISTRATION,
                        payload["id"].as_str().ok_or(Bad)?,
                    )?
                    .is_none()
                {
                    return Err(Bad);
                }
            }
            CODE_RESET_PROGRESS => {
                let reset = payload["resetId"].as_str().ok_or(Bad)?;
                let record = payload["recordId"].as_str().ok_or(Bad)?;
                let parent: CodeResetJournal = self.code_parent(CODE_RESET, reset)?.ok_or(Bad)?;
                self.validate_code_binding_publication(&parent.preview.binding)?;
                if !parent.preview.target_ids.iter().any(|v| v == record) {
                    return Err(Bad);
                }
            }
            _ => (),
        }
        Ok(())
    }
    pub(crate) fn code_get<T: DeserializeOwned>(
        &self,
        family: &str,
        id: &str,
    ) -> CoreResult<Option<T>> {
        self.get::<Value>(family, id)
            .map_err(core)?
            .map(|v| {
                self.validate_code_parents(family, &v).map_err(core)?;
                decode(v["record"].clone()).map_err(core)
            })
            .transpose()
    }
    pub(crate) fn code_all<T: DeserializeOwned>(&self, family: &str) -> CoreResult<Vec<T>> {
        self.select_values(family, None)
            .map_err(core)?
            .into_iter()
            .map(|v| {
                self.validate_code_parents(family, &v).map_err(core)?;
                decode(v["record"].clone()).map_err(core)
            })
            .collect()
    }
    pub(crate) fn code_put<T: Serialize + Clone>(
        &self,
        family: &str,
        id: &str,
        record: &T,
        extra: Value,
    ) -> CoreResult<T> {
        let mut envelope = json!({"id":id,"scope":self.inner.partition.scope,"record":record});
        for (k, v) in extra.as_object().ok_or_else(invalid)? {
            envelope[k] = v.clone();
        }
        self.stage(family, id, Some(envelope)).map_err(core)?;
        Ok(record.clone())
    }
    pub(crate) fn code_fence(&self, fence: Fence) -> CoreResult<()> {
        let session = self.session.as_ref().ok_or_else(changed)?;
        let mut state = session.pending.lock().map_err(|_| changed())?;
        let pending = state
            .as_mut()
            .filter(|p| self.step_id.as_ref() == Some(&p.id))
            .ok_or_else(changed)?;
        pending.fences.push(fence);
        Ok(())
    }
    pub(crate) fn code_expect(
        &self,
        family: &str,
        id: &str,
        old: Option<Value>,
        reason: &'static str,
    ) -> CoreResult<()> {
        let key = self.inner.partition.record_key(family, id).map_err(core)?;
        let mut predicate = vec![SourcePredicate::eq("key", key)];
        let empty = old.is_none();
        if let Some(old) = old {
            predicate.push(SourcePredicate::eq(
                "payload",
                serde_json::to_string(&old).map_err(|_| invalid())?,
            ));
        }
        self.code_fence(Fence {
            family: family.into(),
            predicate: SourcePredicate::and(predicate),
            minimum: 1,
            empty,
            reason,
        })
    }
    pub(crate) fn code_write<T>(
        &self,
        work: impl FnOnce(&HelixStore) -> CoreResult<T>,
    ) -> CoreResult<T> {
        self.domain_atomic(|s| {
            let initialized = s
                .session
                .as_ref()
                .ok_or_else(changed)?
                .pending
                .lock()
                .map_err(|_| changed())?
                .as_ref()
                .ok_or_else(changed)?
                .mutations
                .contains_key(&(CODE_FENCE.into(), "code-writer".into()));
            if !initialized {
                let old = s.get::<Value>(CODE_FENCE, "code-writer").map_err(core)?;
                let revision = old.as_ref().and_then(|v| v["record"].as_u64()).unwrap_or(0);
                s.code_expect(CODE_FENCE, "code-writer", old, "GenerationChanged")?;
                s.code_put(CODE_FENCE, "code-writer", &increment(revision)?, json!({}))?;
            }
            work(s)
        })
    }
    pub(crate) fn code_bound(&self, repo: &RepoId) -> CoreResult<CodeBinding> {
        self.code_get(CODE_BINDING, &repo.0)?.ok_or_else(|| {
            engram_knowledge::code_storage_policy::code_error(CodeBindingError::BindingRequired)
        })
    }
    pub(crate) fn code_attempt(&self, repo: &RepoId, id: &str) -> CoreResult<Option<AttemptRow>> {
        Ok(self
            .code_get::<AttemptRow>(CODE_ATTEMPT, id)?
            .filter(|r| r.repo == *repo))
    }
    pub(crate) fn code_epoch(&self) -> CoreResult<u64> {
        Ok(self.code_get(CODE_SCOPE, "scope")?.unwrap_or(0))
    }
    pub(crate) fn advance_code_epoch(&self) -> CoreResult<()> {
        self.code_put(
            CODE_SCOPE,
            "scope",
            &increment(self.code_epoch()?)?,
            json!({}),
        )?;
        Ok(())
    }
    pub(crate) fn code_view(
        &self,
        include_lineage: bool,
    ) -> CoreResult<(CodeEligibility, Vec<CodeBinding>, Vec<TargetLineage>)> {
        self.admit_logical_work().map_err(core)?;
        let staged = self
            .session
            .as_ref()
            .is_some_and(|s| s.pending.lock().is_ok_and(|p| p.is_some()));
        let (mut bindings, epoch, lineages) = if staged {
            (
                self.code_all::<CodeBinding>(CODE_BINDING)?,
                self.code_epoch()?,
                if include_lineage {
                    self.code_all::<TargetLineage>(CODE_LINEAGE)?
                } else {
                    vec![]
                },
            )
        } else {
            let mut batch = read_batch();
            let mut families = vec![("bindings", CODE_BINDING), ("epoch", CODE_SCOPE)];
            if include_lineage {
                families.push(("lineages", CODE_LINEAGE));
            }
            for (name, family) in &families {
                batch = batch.var_as(
                    name,
                    g().n_with_label_where(
                        *family,
                        SourcePredicate::eq(
                            "partition",
                            self.inner.partition.discriminator().map_err(core)?,
                        ),
                    )
                    .value_map(None::<Vec<String>>),
                );
            }
            let response = self
                .inner
                .transport
                .execute(
                    &crate::transport::Operation::new(QueryRequest::read(
                        batch.returning(families.iter().map(|(n, _)| *n)),
                    ))
                    .map_err(core)?,
                )
                .map_err(core)?;
            let mut bindings = vec![];
            for row in crate::schema::rows(&response, "bindings").map_err(core)? {
                let payload = self.checked_payload(CODE_BINDING, row).map_err(core)?;
                self.validate_code_parents(CODE_BINDING, &payload)
                    .map_err(core)?;
                bindings.push(decode::<CodeBinding>(payload["record"].clone()).map_err(core)?);
            }
            let epochs = crate::schema::rows(&response, "epoch").map_err(core)?;
            if epochs.len() > 1 {
                return Err(core(crate::error::Error::IncompatibleSchema));
            }
            let epoch = epochs
                .first()
                .map(|row| {
                    self.checked_payload(CODE_SCOPE, row)
                        .and_then(|p| decode::<u64>(p["record"].clone()))
                })
                .transpose()
                .map_err(core)?
                .unwrap_or(0);
            let mut lineages = vec![];
            if include_lineage {
                for row in crate::schema::rows(&response, "lineages").map_err(core)? {
                    let payload = self.checked_payload(CODE_LINEAGE, row).map_err(core)?;
                    self.validate_code_parents(CODE_LINEAGE, &payload)
                        .map_err(core)?;
                    lineages
                        .push(decode::<TargetLineage>(payload["record"].clone()).map_err(core)?);
                }
            }
            (bindings, epoch, lineages)
        };
        bindings.sort_by(|a, b| a.repo_id.0.cmp(&b.repo_id.0));
        let mut view = CodeEligibility {
            scope: self.inner.partition.scope.clone(),
            epoch,
            generations: vec![],
            repository_ids: vec![],
        };
        for b in &bindings {
            if let Some(g) = &b.active_generation {
                view.repository_ids.push(b.repo_id.0.clone());
                view.generations.push(g.clone());
            }
        }
        Ok((view, bindings, lineages))
    }
    pub(crate) fn code_eligibility(&self) -> CoreResult<CodeEligibility> {
        Ok(self.code_view(false)?.0)
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AttemptRow {
    pub(crate) repo: RepoId,
    pub(crate) generation: CodeGeneration,
    pub(crate) attempt: CodeAttempt,
}
#[async_trait]
impl CodeBindingRepository for HelixStore {
    async fn register(&self, r: RepositoryRegistration) -> CoreResult<RepositoryRegistration> {
        if r.repo_id.0.trim().is_empty() || r.locator_id.trim().is_empty() {
            return Err(invalid());
        }
        self.code_write(|s| {
            if let Some(old) =
                s.code_get::<RepositoryRegistration>(CODE_REGISTRATION, &r.repo_id.0)?
            {
                if old != r {
                    return Err(CoreError::Conflict {
                        reason: "BindingConflict".into(),
                    });
                }
                return Ok(old);
            }
            s.code_put(
                CODE_REGISTRATION,
                &r.repo_id.0,
                &r,
                json!({"repoId":r.repo_id}),
            )
        })
    }
    async fn get_registration(&self, repo: &RepoId) -> CoreResult<Option<RepositoryRegistration>> {
        self.code_get(CODE_REGISTRATION, &repo.0)
    }
    async fn bind(&self, b: CodeBinding) -> CoreResult<CodeBinding> {
        b.validate()
            .map_err(engram_knowledge::code_storage_policy::code_error)?;
        self.check_scope(&b.scope).map_err(core)?;
        self.code_write(|s| {
            if s.code_get::<RepositoryRegistration>(CODE_REGISTRATION, &b.repo_id.0)?
                .is_none()
            {
                return Err(invalid());
            }
            if let Some(old) = s.code_get::<CodeBinding>(CODE_BINDING, &b.repo_id.0)? {
                if old.branch_ref != b.branch_ref {
                    return Err(CoreError::Conflict {
                        reason: "BindingConflict".into(),
                    });
                }
                return Ok(old);
            }
            if b.version != 1 || b.epoch != 0 || b.active_generation.is_some() {
                return Err(invalid());
            }
            s.advance_code_epoch()?;
            s.code_put(
                CODE_BINDING,
                &b.repo_id.0,
                &b,
                json!({"repoId":b.repo_id,"epoch_key":b.epoch.to_string()}),
            )
        })
    }
    async fn get_binding(&self, scope: &Scope, repo: &RepoId) -> CoreResult<Option<CodeBinding>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.code_get(CODE_BINDING, &repo.0)
    }
    async fn bindings(&self, scope: &Scope) -> CoreResult<Vec<CodeBinding>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.code_all(CODE_BINDING)
    }
    async fn begin_attempt(
        &self,
        scope: &Scope,
        repo: &RepoId,
        g: CodeGeneration,
        mut a: CodeAttempt,
    ) -> CoreResult<CodeAttempt> {
        self.check_scope(scope).map_err(core)?;
        g.validate().map_err(|_| invalid())?;
        self.code_write(|s|{let binding=s.code_bound(repo)?;let rows=s.code_all::<AttemptRow>(CODE_ATTEMPT)?;if let Some(prior)=rows.iter().find(|r|r.repo==*repo&&r.attempt.idempotency_key==a.idempotency_key){if a.fencing_token==0{a.fencing_token=prior.attempt.fencing_token;}if prior.attempt!=a||prior.generation!=g{return Err(invalid());}return Ok(a);}if binding.version!=a.binding_version||g.binding_version!=a.binding_version||binding.epoch!=a.expected_epoch||binding.active_generation!=a.expected_active_generation{return Err(changed());}let latest=rows.iter().filter(|r|r.repo==*repo).map(|r|r.attempt.fencing_token).max().unwrap_or(0);if a.fencing_token==0{a.fencing_token=increment(latest)?;}a.validate().map_err(|_|invalid())?;if a.fencing_token<=latest||s.code_get::<AttemptRow>(CODE_ATTEMPT,&a.id)?.is_some(){return Err(invalid());}s.code_immutable_generation(&g)?;s.code_put(CODE_GENERATION_OWNER,&tuple(&[&repo.0,&g.id]),&g,json!({"repoId":repo,"generationId":g.id}))?;let row=AttemptRow{repo:repo.clone(),generation:g.clone(),attempt:a.clone()};s.code_put(CODE_ATTEMPT,&a.id,&row,json!({"repoId":repo,"generationId":g.id,"fencing_key":a.fencing_token.to_string()}))?;Ok(a.clone())})
    }
    async fn get_attempt(
        &self,
        scope: &Scope,
        repo: &RepoId,
        id: &str,
    ) -> CoreResult<Option<(CodeGeneration, CodeAttempt)>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        Ok(self
            .code_attempt(repo, id)?
            .map(|r| (r.generation, r.attempt)))
    }
    async fn get_generation(
        &self,
        scope: &Scope,
        repo: &RepoId,
        id: &str,
    ) -> CoreResult<Option<CodeGeneration>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.code_get(CODE_GENERATION_OWNER, &tuple(&[&repo.0, id]))
    }

    async fn put_generation(&self, g: CodeGeneration) -> CoreResult<CodeGeneration> {
        self.code_write(|s| {
            s.code_immutable_generation(&g)?;
            Ok(g.clone())
        })
    }
    async fn put_lineage(
        &self,
        scope: &Scope,
        id: &str,
        l: CodeLineage,
    ) -> CoreResult<CodeLineage> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s| {
            s.code_stage_lineage(id, &l)?;
            Ok(l.clone())
        })
    }
    async fn target_lineage(&self, scope: &Scope) -> CoreResult<Vec<TargetLineage>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.code_all(CODE_LINEAGE)
    }
    async fn record_lineage(&self, scope: &Scope, id: &str) -> CoreResult<Option<TargetLineage>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.code_get(CODE_LINEAGE, id)
    }
    async fn put_source_descriptor(
        &self,
        scope: &Scope,
        src: IndexedSource,
    ) -> CoreResult<IndexedSource> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s| {
            s.code_stage_source(&src)?;
            Ok(src.clone())
        })
    }
    async fn get_source_descriptor(
        &self,
        scope: &Scope,
        repo: &RepoId,
        g: &str,
        path: &str,
    ) -> CoreResult<Option<IndexedSource>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.code_get(CODE_SOURCE, &tuple(&[&repo.0, g, path]))
    }
    async fn put_ref_observation(
        &self,
        scope: &Scope,
        repo: &RepoId,
        o: CodeRefObservation,
    ) -> CoreResult<CodeRefObservation> {
        self.check_scope(scope).map_err(core)?;
        o.validate().map_err(|_| invalid())?;
        self.code_write(|s| {
            let binding = s.code_bound(repo)?;
            if binding.branch_ref != o.branch_ref {
                return Err(changed());
            }
            s.code_put(CODE_OBSERVATION, &repo.0, &o, json!({"repoId":repo}))
        })
    }
    async fn get_ref_observation(
        &self,
        scope: &Scope,
        repo: &RepoId,
    ) -> CoreResult<Option<CodeRefObservation>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.code_get(CODE_OBSERVATION, &repo.0)
    }
    async fn record_receipt(&self, r: StageReceipt) -> CoreResult<StageReceipt> {
        self.code_write(|s| {
            let row = s
                .code_get::<AttemptRow>(CODE_ATTEMPT, &r.attempt_id)?
                .ok_or_else(invalid)?;
            let lanes = row
                .attempt
                .receipt_kinds
                .iter()
                .map(|v| {
                    serde_json::to_value(v)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect::<Vec<_>>();
            if row.generation.id != r.generation_id
                || !lanes.contains(&r.lane)
                || r.digest.is_empty()
            {
                return Err(invalid());
            }
            let id = tuple(&[&r.attempt_id, &r.lane]);
            if let Some(old) = s.code_get::<StageReceipt>(CODE_STAGE_RECEIPT, &id)? {
                if old != r {
                    return Err(invalid());
                }
                return Ok(old);
            }
            s.code_put(
                CODE_STAGE_RECEIPT,
                &id,
                &r,
                json!({"attemptId":r.attempt_id,"generationId":r.generation_id}),
            )
        })
    }
    async fn mark_ready(
        &self,
        scope: &Scope,
        repo: &RepoId,
        id: &str,
        fence: u64,
    ) -> CoreResult<CodeAttempt> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s|{let mut row=s.code_attempt(repo,id)?.ok_or_else(changed)?;s.check_attempt_ready(&row,fence)?;if !matches!(row.attempt.state,CodeAttemptState::Created|CodeAttemptState::Acquiring|CodeAttemptState::Staging|CodeAttemptState::Ready){return Err(changed());}row.attempt.state=CodeAttemptState::Ready;s.code_put(CODE_ATTEMPT,id,&row,json!({"repoId":repo,"generationId":row.generation.id,"fencing_key":fence.to_string()}))?;Ok(row.attempt)})
    }
    async fn publish(
        &self,
        scope: &Scope,
        repo: &RepoId,
        id: &str,
        epoch: u64,
    ) -> CoreResult<CodeBinding> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s|{let mut row=s.code_attempt(repo,id)?.ok_or_else(changed)?;if row.attempt.expected_epoch!=epoch{return Err(changed());}let mut binding=s.code_bound(repo)?;if row.attempt.state==CodeAttemptState::Published&&binding.active_generation.as_deref()==Some(&row.generation.id)&&binding.epoch==increment(row.attempt.expected_epoch)?&&binding.version==row.attempt.binding_version{return Ok(binding);}s.check_attempt_ready(&row,row.attempt.fencing_token)?;if row.attempt.expected_epoch!=epoch||binding.epoch!=epoch||binding.version!=row.attempt.binding_version||binding.active_generation!=row.attempt.expected_active_generation||row.attempt.state!=CodeAttemptState::Ready{return Err(changed());}let old=s.get::<Value>(CODE_BINDING,&repo.0).map_err(core)?;s.code_expect(CODE_BINDING,&repo.0,old,"GenerationChanged")?;binding.epoch=increment(binding.epoch)?;binding.active_generation=Some(row.generation.id.clone());row.attempt.state=CodeAttemptState::Published;s.code_put(CODE_BINDING,&repo.0,&binding,json!({"repoId":repo,"epoch_key":binding.epoch.to_string()}))?;s.code_put(CODE_ATTEMPT,id,&row,json!({"repoId":repo,"generationId":row.generation.id,"fencing_key":row.attempt.fencing_token.to_string()}))?;s.advance_code_epoch()?;Ok(binding)})
    }
    async fn eligibility(&self, scope: &Scope) -> CoreResult<CodeEligibility> {
        if !self.visible_scope(scope) {
            return Err(changed());
        }
        self.code_eligibility()
    }
    async fn preview_reset(&self, scope: &Scope, repo: &RepoId) -> CoreResult<CodeResetPreview> {
        code_storage_policy::preview_reset(self, scope, repo, "helix-knowledge").await
    }
    async fn journal_reset(&self, j: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        let j = code_storage_policy::prepare_reset_journal(self, j).await?;
        self.code_write(|s| {
            let id = &j.reset_id;
            if let Some(old) = s.code_get::<CodeResetJournal>(CODE_RESET, id)? {
                if old != j {
                    return Err(invalid());
                }
                return Ok(old);
            }
            s.code_put(
                CODE_RESET,
                id,
                &j,
                json!({"repoId":j.preview.binding.repo_id}),
            )
        })
    }
    async fn load_reset_journal(&self, id: &str) -> CoreResult<CodeResetJournal> {
        let mut j: CodeResetJournal =
            self.code_get(CODE_RESET, id)?
                .ok_or_else(|| CoreError::Conflict {
                    reason:
                        "ResetJournalUnauthorized: no owner-approved journal exists for reset id"
                            .into(),
                })?;
        for p in self
            .select_values(CODE_RESET_PROGRESS, Some(("resetId", id)))
            .map_err(core)?
        {
            self.validate_code_parents(CODE_RESET_PROGRESS, &p)
                .map_err(core)?;
            let checkpoint = p["record"].as_str().ok_or_else(invalid)?.to_owned();
            if !j.checkpoints.contains(&checkpoint) {
                j.checkpoints.push(checkpoint);
            }
        }
        Ok(j)
    }
    async fn save_reset_journal(&self, j: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        j.validate().map_err(|_| invalid())?;
        self.code_write(|s| {
            let old: CodeResetJournal = s.code_get(CODE_RESET, &j.reset_id)?.ok_or_else(changed)?;
            let mut a = old.clone();
            let mut b = j.clone();
            a.checkpoints.clear();
            b.checkpoints.clear();
            if a != b {
                return Err(changed());
            }
            s.code_put(
                CODE_RESET,
                &j.reset_id,
                &j,
                json!({"repoId":j.preview.binding.repo_id}),
            )
        })
    }
    async fn retire_lineage_targets(
        &self,
        scope: &Scope,
        epochs: &[(RepoId, u64)],
        targets: &[RetirementTarget],
    ) -> CoreResult<()> {
        self.retire_code(scope, epochs, targets, None)
    }
    async fn retire_lineage_targets_with_reset_progress(
        &self,
        scope: &Scope,
        epochs: &[(RepoId, u64)],
        targets: &[RetirementTarget],
        progress: Option<(&str, &[String])>,
    ) -> CoreResult<()> {
        self.retire_code(scope, epochs, targets, progress)
    }
}
impl HelixStore {
    fn check_attempt_ready(&self, row: &AttemptRow, fence: u64) -> CoreResult<()> {
        let latest = self
            .code_all::<AttemptRow>(CODE_ATTEMPT)?
            .into_iter()
            .filter(|r| r.repo == row.repo)
            .map(|r| r.attempt.fencing_token)
            .max()
            .unwrap_or(0);
        let mut declared = row
            .attempt
            .receipt_kinds
            .iter()
            .map(|v| {
                serde_json::to_value(v)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let mut actual = self
            .code_all::<StageReceipt>(CODE_STAGE_RECEIPT)?
            .into_iter()
            .filter(|r| r.attempt_id == row.attempt.id && r.generation_id == row.generation.id)
            .map(|r| r.lane)
            .collect::<Vec<_>>();
        declared.sort();
        actual.sort();
        if fence != latest
            || fence != row.attempt.fencing_token
            || declared.is_empty()
            || declared != actual
        {
            return Err(changed());
        }
        Ok(())
    }
}
#[async_trait]
impl CodeEligibilityGate for HelixStore {
    async fn authorize_query(
        &self,
        scope: &Scope,
        requested: &[String],
        limit: usize,
    ) -> CoreResult<ConstrainedCodeQuery> {
        code_storage_policy::query_budget(limit)?;
        if !self.visible_scope(scope) {
            return Err(changed());
        }
        let (view, bindings, lineages) = self.code_view(true)?;
        let mut ids = lineages
            .into_iter()
            .filter(|l| {
                !l.retired
                    && bindings.iter().any(|b| {
                        b.repo_id.0 == l.repo_id
                            && b.active_generation.as_deref() == Some(&l.generation_id)
                    })
            })
            .map(|l| l.record_id)
            .collect::<Vec<_>>();
        ids.sort();
        code_storage_policy::constrain_query(view, requested, &ids, limit)
    }
    async fn authorize_maintenance(
        &self,
        scope: &Scope,
        ids: &[String],
    ) -> CoreResult<Vec<String>> {
        code_storage_policy::explicit_maintenance(ids)?;
        Ok(self
            .authorize_query(scope, ids, ids.len())
            .await?
            .target_ids)
    }
}

#[cfg(all(test, any(feature = "embedded", feature = "service")))]
mod code_integrity_controls {
    use super::*;
    use crate::{test_data_fixture::*, transport::Operation};
    use futures::executor::block_on;
    use std::collections::{BTreeMap, BTreeSet};
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Kind {
        String,
        U64,
    }
    impl Kind {
        fn accepts(self, v: &Value) -> bool {
            match self {
                Self::String => v.is_string(),
                Self::U64 => v.as_u64().is_some(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Phase {
        BeforeRetirement,
        AfterRetirement,
    }
    struct Case {
        family: &'static str,
        id: String,
        path: &'static str,
        replacement: Value,
        projection: bool,
        kind: Kind,
        optional_subject: bool,
        phase: Phase,
    }
    struct Witness {
        id: String,
        envelope: Value,
        phase: Phase,
    }
    fn envelope<T: Serialize + DeserializeOwned>(
        id: &str,
        scope: &Scope,
        record: &T,
        extra: Value,
    ) -> Result<Value, String> {
        let body = serde_json::to_value(record).map_err(|e| e.to_string())?;
        let roundtrip: T = serde_json::from_value(body.clone()).map_err(|e| e.to_string())?;
        if serde_json::to_value(roundtrip).map_err(|e| e.to_string())? != body {
            return Err("untouched typed roundtrip differs".into());
        }
        let mut payload = json!({"id":id,"scope":scope,"record":body});
        for (key, value) in extra
            .as_object()
            .ok_or("typed witness extras must be an object")?
        {
            payload[key] = value.clone();
        }
        Ok(payload)
    }
    // Schema witnesses are never stored or used as authority. Canonical typed serialization
    // supplies field spelling/types; canonical hash/validate methods avoid policy copies.
    fn typed_witnesses(expires: Timestamp) -> (BTreeMap<&'static str, Witness>, Vec<String>) {
        let mut result = BTreeMap::new();
        let mut errors = Vec::new();
        let scope = Scope {
            subject: Some("schema-subject".into()),
            ..scope()
        };
        let repo = RepoId("repo".into());
        let registration = RepositoryRegistration {
            repo_id: repo.clone(),
            locator_id: "fixture-locator".into(),
            acquisition: AcquisitionMode::LocalOnly,
        };
        let binding = CodeBinding {
            scope: scope.clone(),
            repo_id: repo.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: Some("g1".into()),
            epoch: 1,
        };
        let generation = code_generation("g1");
        let mut attempt = code_attempt("a1", 0, None);
        attempt.fencing_token = 1;
        attempt.state = CodeAttemptState::Published;
        let attempt_row = AttemptRow {
            repo: repo.clone(),
            generation: generation.clone(),
            attempt: attempt.clone(),
        };
        let source_lineage = code_lineage("repo", "g1");
        let lineage = TargetLineage {
            record_id: "first".into(),
            repo_id: repo.0.clone(),
            generation_id: generation.id.clone(),
            retired: false,
            source_path: source_lineage.source_path.clone(),
            span: source_lineage.span,
            contributors: source_lineage.contributors.clone(),
        };
        let source = IndexedSource {
            repo_id: repo.clone(),
            generation_id: generation.id.clone(),
            commit_oid: generation.commit_oid.clone(),
            path: "src/lib.rs".into(),
            content_digest: "3".repeat(64),
        };
        let receipt = StageReceipt {
            attempt_id: attempt.id.clone(),
            generation_id: generation.id.clone(),
            lane: "source".into(),
            digest: "schema-digest".into(),
        };
        let lease = CodeReadLease {
            id: "lease".into(),
            eligibility: CodeEligibility {
                scope: scope.clone(),
                epoch: 2,
                generations: vec![generation.id.clone()],
                repository_ids: vec![repo.0.clone()],
            },
            fencing_token: 1,
            expires_at: expires,
        };
        let observation = CodeRefObservation {
            branch_ref: binding.branch_ref.clone(),
            observed_oid: "4".repeat(40),
            observed_at: expires,
            policy: RefObservationPolicy::OnRequest,
        };
        let inventory = PreservationInventory::default();
        let fingerprint = "a".repeat(64);
        let target = CodeResetTarget {
            record_id: lineage.record_id.clone(),
            store_id: "helix-knowledge".into(),
            fingerprint: fingerprint.clone(),
            lineage: source_lineage.clone(),
            snapshot_ref: generation.snapshot_digest.clone(),
            scan_metadata_keys: vec!["bindingVersion".into(), "codeEpoch".into()],
            binding_version: 1,
            code_epoch: 1,
        };
        let mut preview = CodeResetPreview {
            binding: binding.clone(),
            preview_hash: String::new(),
            target_ids: vec![lineage.record_id.clone()],
            targets: vec![target],
            ambiguity: vec![],
            preservation_digests: vec![inventory.digest.clone()],
            preservation_inventory: inventory.clone(),
            binding_versions: vec![(repo.clone(), 1)],
            code_epoch: 2,
        };
        match preview.content_hash() {
            Ok(hash) => preview.preview_hash = hash,
            Err(error) => errors.push(format!("reset witness canonical hash: {error:?}")),
        }
        let journal = CodeResetJournal {
            reset_id: "reset".into(),
            preview_hash: preview.preview_hash.clone(),
            preview,
            target_fingerprints: vec![(lineage.record_id.clone(), fingerprint)],
            authorizing_decisions: vec!["schema-witness-not-authority".into()],
            checkpoints: vec![],
            preservation_digests: vec![inventory.digest],
        };
        for (family, validation) in [
            (CODE_BINDING, binding.validate()),
            (CODE_GENERATION, generation.validate()),
            (CODE_ATTEMPT, attempt.validate()),
            (CODE_LINEAGE, source_lineage.validate()),
            (CODE_SOURCE, source.validate()),
            (CODE_OBSERVATION, observation.validate()),
            (CODE_RESET, journal.validate()),
        ] {
            if let Err(error) = validation {
                errors.push(format!(
                    "{family}: untouched canonical type invalid: {error:?}"
                ));
            }
        }
        macro_rules! add {
            ($family:expr,$id:expr,$record:expr,$extra:expr,$phase:expr) => {{
                let id: String = $id;
                match envelope(&id, &scope, $record, $extra) {
                    Ok(value) => {
                        result.insert(
                            $family,
                            Witness {
                                id,
                                envelope: value,
                                phase: $phase,
                            },
                        );
                    }
                    Err(error) => {
                        errors.push(format!("{}: untouched typed witness: {error}", $family))
                    }
                }
            }};
        }
        add!(
            CODE_REGISTRATION,
            registration.repo_id.0.clone(),
            &registration,
            json!({"repoId":registration.repo_id}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_BINDING,
            binding.repo_id.0.clone(),
            &binding,
            json!({"repoId":binding.repo_id,"epoch_key":binding.epoch.to_string()}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_GENERATION,
            generation.id.clone(),
            &generation,
            json!({"generationId":generation.id}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_GENERATION_OWNER,
            tuple(&[&repo.0, &generation.id]),
            &generation,
            json!({"repoId":repo,"generationId":generation.id}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_ATTEMPT,
            attempt.id.clone(),
            &attempt_row,
            json!({"repoId":repo,"generationId":generation.id,"fencing_key":attempt.fencing_token.to_string()}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_LINEAGE,
            lineage.record_id.clone(),
            &lineage,
            json!({"repoId":lineage.repo_id,"generationId":lineage.generation_id,"retired":lineage.retired}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_SOURCE,
            tuple(&[&repo.0, &generation.id, &source.path]),
            &source,
            json!({"repoId":repo,"generationId":generation.id,"path":source.path}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_STAGE_RECEIPT,
            tuple(&[&attempt.id, &receipt.lane]),
            &receipt,
            json!({"attemptId":attempt.id,"generationId":generation.id}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_SCOPE,
            "scope".into(),
            &2_u64,
            json!({}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_FENCE,
            "code-writer".into(),
            &1_u64,
            json!({}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_LEASE,
            lease.id.clone(),
            &lease,
            json!({"fence_key":lease.fencing_token.to_string(),"expires_ms":expires.timestamp_millis(),"expires_secs":expires.timestamp(),"expires_nanos":expires.timestamp_subsec_nanos()}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_LEASE_COVERAGE,
            tuple(&[&lease.id, &generation.id]),
            &lease.id,
            json!({"leaseId":lease.id,"generationId":generation.id,"expires_ms":expires.timestamp_millis(),"expires_secs":expires.timestamp(),"expires_nanos":expires.timestamp_subsec_nanos()}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_LEASE_TOMBSTONE,
            "auxiliary".into(),
            &"auxiliary".to_owned(),
            json!({}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_RESET,
            journal.reset_id.clone(),
            &journal,
            json!({"repoId":repo}),
            Phase::BeforeRetirement
        );
        add!(
            CODE_RESET_PROGRESS,
            tuple(&[&journal.reset_id, &lineage.record_id]),
            &"completed:first".to_owned(),
            json!({"resetId":journal.reset_id,"recordId":lineage.record_id}),
            Phase::AfterRetirement
        );
        add!(
            CODE_OBSERVATION,
            repo.0.clone(),
            &observation,
            json!({"repoId":repo}),
            Phase::BeforeRetirement
        );
        (result, errors)
    }
    fn kind(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
    fn preflight(cases: &[Case], expires: Timestamp) -> Vec<String> {
        let (witnesses, mut errors) = typed_witnesses(expires);
        let expected = BTreeSet::from([
            CODE_REGISTRATION,
            CODE_BINDING,
            CODE_GENERATION,
            CODE_GENERATION_OWNER,
            CODE_ATTEMPT,
            CODE_LINEAGE,
            CODE_SOURCE,
            CODE_STAGE_RECEIPT,
            CODE_SCOPE,
            CODE_FENCE,
            CODE_LEASE,
            CODE_LEASE_COVERAGE,
            CODE_LEASE_TOMBSTONE,
            CODE_RESET,
            CODE_RESET_PROGRESS,
            CODE_OBSERVATION,
        ]);
        let actual = cases.iter().map(|c| c.family).collect::<BTreeSet<_>>();
        if actual != expected {
            errors.push(format!(
                "family coverage missing={:?} unexpected={:?}",
                expected.difference(&actual).collect::<Vec<_>>(),
                actual.difference(&expected).collect::<Vec<_>>()
            ));
        }
        if cases.len() != 28 {
            errors.push(format!(
                "case coverage expected27+progress=28 actual={}",
                cases.len()
            ));
        }
        if cases
            .iter()
            .filter(|c| c.phase == Phase::BeforeRetirement)
            .count()
            != 27
            || cases
                .iter()
                .filter(|c| c.phase == Phase::AfterRetirement)
                .count()
                != 1
        {
            errors.push(
                "phase coverage requires27 before-retirement and1 reset-progress after-retirement"
                    .into(),
            );
        }
        for case in cases {
            let context = format!(
                "family={} id={} path={} phase={:?} expected={:?}",
                case.family, case.id, case.path, case.phase, case.kind
            );
            let Some(w) = witnesses.get(case.family) else {
                errors.push(format!("{context}: no typed canonical witness"));
                continue;
            };
            if case.id != w.id || w.envelope["id"] != json!(case.id) {
                errors.push(format!(
                    "{context}: identity differs from typed canonical id {}",
                    w.id
                ));
            }
            if case.phase != w.phase {
                errors.push(format!("{context}: wrong required phase {:?}", w.phase));
            }
            let parent = if case.projection {
                Some(&w.envelope)
            } else {
                case.path
                    .rsplit_once('/')
                    .and_then(|(p, _)| w.envelope.pointer(p))
            };
            let available = parent
                .and_then(Value::as_object)
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let original = if case.projection {
                w.envelope.get(case.path)
            } else {
                w.envelope.pointer(case.path)
            };
            match original {
                None => errors.push(format!(
                    "{context}: missing canonical serde path; available={available:?}"
                )),
                Some(value) => {
                    if !case.kind.accepts(value) {
                        errors.push(format!(
                            "{context}: observed={} available={available:?}",
                            kind(value)
                        ));
                    }
                    if value == &case.replacement {
                        errors.push(format!("{context}: replacement is a no-op"));
                    }
                }
            }
            let deliberate_invalid_type = matches!(case.family, CODE_SCOPE | CODE_FENCE)
                && case.path == "/record"
                && case.replacement == json!("not-u64");
            if !deliberate_invalid_type && !case.kind.accepts(&case.replacement) {
                errors.push(format!(
                    "{context}: unexpected replacement kind {}",
                    kind(&case.replacement)
                ));
            }
            if case.optional_subject
                && (!case.path.ends_with("/scope/subject")
                    || !parent.is_some_and(Value::is_object)
                    || case.kind != Kind::String
                    || case.projection)
            {
                errors.push(format!(
                    "{context}: invalid optional-field insertion parent"
                ));
            }
            if case.path.ends_with("/expiresAt")
                && case
                    .replacement
                    .as_str()
                    .is_none_or(|s| s.parse::<Timestamp>().is_err())
            {
                errors.push(format!("{context}: replacement is not RFC3339 Timestamp"));
            }
        }
        errors
    }
    fn assert_preflight(cases: &[Case], expires: Timestamp) {
        let errors = preflight(cases, expires);
        assert!(
            errors.is_empty(),
            "pure corruption matrix setup errors (no engine acquired):\n{}",
            errors.join("\n")
        );
        eprintln!(
            "TASK006 pure schema preflight:28 cases/16 families; setup only, not provider verification or history reset"
        );
    }
    #[test]
    fn pure_full_corruption_matrix_preflight_before_acquisition() {
        let expires = chrono::Utc::now() + chrono::Duration::hours(1);
        assert_preflight(&corruption_cases(expires), expires);
    }
    fn corruption_cases(expires: Timestamp) -> Vec<Case> {
        [
            (
                CODE_REGISTRATION,
                "repo".into(),
                "/record/repo_id",
                json!("foreign"),
                false,
            ),
            (
                CODE_BINDING,
                "repo".into(),
                "/record/scope/subject",
                json!("foreign"),
                false,
            ),
            (
                CODE_BINDING,
                "repo".into(),
                "/record/repoId",
                json!("foreign"),
                false,
            ),
            (
                CODE_BINDING,
                "repo".into(),
                "/record/epoch",
                json!(99),
                false,
            ),
            (
                CODE_BINDING,
                "repo".into(),
                "/record/activeGeneration",
                json!("g2"),
                false,
            ),
            (
                CODE_BINDING,
                "repo".into(),
                "repoId",
                json!("foreign"),
                true,
            ),
            (
                CODE_GENERATION,
                "g1".into(),
                "/record/id",
                json!("foreign"),
                false,
            ),
            (
                CODE_GENERATION_OWNER,
                tuple(&["repo", "g1"]),
                "/record/id",
                json!("foreign"),
                false,
            ),
            (
                CODE_GENERATION_OWNER,
                tuple(&["repo", "g1"]),
                "/record/parserFingerprint",
                json!("substituted"),
                false,
            ),
            (
                CODE_ATTEMPT,
                "a1".into(),
                "/record/attempt/id",
                json!("foreign"),
                false,
            ),
            (
                CODE_ATTEMPT,
                "a1".into(),
                "/record/repo",
                json!("foreign"),
                false,
            ),
            (
                CODE_ATTEMPT,
                "a1".into(),
                "/record/attempt/fencingToken",
                json!(99),
                false,
            ),
            (
                CODE_LINEAGE,
                "first".into(),
                "/record/recordId",
                json!("substituted-effect-target"),
                false,
            ),
            (
                CODE_LINEAGE,
                "first".into(),
                "/record/generationId",
                json!("g2"),
                false,
            ),
            (
                CODE_SOURCE,
                tuple(&["repo", "g1", "src/lib.rs"]),
                "/record/generationId",
                json!("g2"),
                false,
            ),
            (
                CODE_SOURCE,
                tuple(&["repo", "g1", "src/lib.rs"]),
                "/record/commitOid",
                json!("9".repeat(40)),
                false,
            ),
            (
                CODE_STAGE_RECEIPT,
                tuple(&["a1", "source"]),
                "/record/attemptId",
                json!("a2"),
                false,
            ),
            (
                CODE_SCOPE,
                "scope".into(),
                "/record",
                json!("not-u64"),
                false,
            ),
            (
                CODE_FENCE,
                "code-writer".into(),
                "/record",
                json!("not-u64"),
                false,
            ),
            (
                CODE_LEASE,
                "lease".into(),
                "/record/id",
                json!("foreign"),
                false,
            ),
            (
                CODE_LEASE,
                "lease".into(),
                "/record/fencingToken",
                json!(99),
                false,
            ),
            (
                CODE_LEASE,
                "lease".into(),
                "/record/eligibility/scope/subject",
                json!("foreign"),
                false,
            ),
            (
                CODE_LEASE,
                "lease".into(),
                "/record/expiresAt",
                json!((expires + chrono::Duration::seconds(1)).to_rfc3339()),
                false,
            ),
            (
                CODE_LEASE_COVERAGE,
                tuple(&["lease", "g1"]),
                "/record",
                json!("foreign"),
                false,
            ),
            (
                CODE_LEASE_TOMBSTONE,
                "auxiliary".into(),
                "/record",
                json!("foreign"),
                false,
            ),
            (
                CODE_RESET,
                "reset".into(),
                "/record/resetId",
                json!("foreign"),
                false,
            ),
            (
                CODE_OBSERVATION,
                "repo".into(),
                "repoId",
                json!("foreign"),
                true,
            ),
            (
                CODE_RESET_PROGRESS,
                tuple(&["reset", "first"]),
                "/record",
                json!("completed:substituted"),
                false,
            ),
        ]
        .into_iter()
        .map(|(family, id, path, replacement, projection)| {
            let kind = if matches!(
                (family, path),
                (CODE_BINDING, "/record/epoch")
                    | (CODE_ATTEMPT, "/record/attempt/fencingToken")
                    | (CODE_SCOPE, "/record")
                    | (CODE_FENCE, "/record")
                    | (CODE_LEASE, "/record/fencingToken")
            ) {
                Kind::U64
            } else {
                Kind::String
            };
            Case {
                family,
                id,
                path,
                replacement,
                projection,
                kind,
                optional_subject: matches!(
                    (family, path),
                    (CODE_BINDING, "/record/scope/subject")
                        | (CODE_LEASE, "/record/eligibility/scope/subject")
                ),
                phase: if family == CODE_RESET_PROGRESS {
                    Phase::AfterRetirement
                } else {
                    Phase::BeforeRetirement
                },
            }
        })
        .collect()
    }
    fn physical(store: &HelixStore, family: &str, id: &str) -> Value {
        let q = read_batch()
            .var_as(
                "rows",
                g().n_with_label_where(
                    family,
                    SourcePredicate::and(vec![
                        SourcePredicate::eq(
                            "partition",
                            store.inner.partition.discriminator().unwrap(),
                        ),
                        SourcePredicate::eq(
                            "key",
                            store.inner.partition.record_key(family, id).unwrap(),
                        ),
                    ]),
                )
                .value_map(None::<Vec<String>>),
            )
            .returning(["rows"]);
        let r = store
            .inner
            .transport
            .execute(&Operation::new(QueryRequest::read(q)).unwrap())
            .unwrap();
        assert_eq!(r["rows"].as_array().unwrap().len(), 1);
        r["rows"][0].clone()
    }
    fn change(
        store: &HelixStore,
        family: &str,
        id: &str,
        payload: &str,
        projection: Option<(&str, &Value)>,
    ) {
        let mut traversal = g()
            .n_with_label_where(
                family,
                SourcePredicate::and(vec![
                    SourcePredicate::eq(
                        "partition",
                        store.inner.partition.discriminator().unwrap(),
                    ),
                    SourcePredicate::eq(
                        "key",
                        store.inner.partition.record_key(family, id).unwrap(),
                    ),
                ]),
            )
            .set_property("payload", payload);
        if let Some((field, value)) = projection {
            traversal = traversal.set_property(
                field,
                value.as_str().expect("declared string projection control"),
            );
        }
        let request = store
            .inner
            .lease
            .as_ref()
            .unwrap()
            .guard()
            .var_as_if(
                "changed",
                BatchCondition::VarMinSize("owner".into(), 1),
                traversal.value_map(Some(vec!["public_id"])),
            )
            .returning(["changed"]);
        let operation = Operation::new(QueryRequest::write(request)).unwrap();
        let result = store.inner.transport.execute(&operation).unwrap();
        assert_eq!(result["changed"].as_array().unwrap().len(), 1);
    }
    async fn reject(store: &HelixStore, case: &Case) {
        let family = case.family;
        let id = case.id.as_str();
        let path = case.path;
        let original = physical(store, family, id);
        let original_text = original["payload"]
            .as_str()
            .expect("actual fixture payload string");
        let mut payload: Value =
            serde_json::from_str(original_text).expect("untouched actual fixture JSON");
        assert!(
            store.code_get::<Value>(family, id).is_ok(),
            "untouched typed fixture invalid family={family} id={id} path={path}"
        );
        if case.projection {
            assert_ne!(
                original.get(path),
                Some(&case.replacement),
                "actual projection mutation must differ: {family}/{id}/{path}"
            );
            change(
                store,
                family,
                id,
                original_text,
                Some((path, &case.replacement)),
            );
        } else {
            if case.optional_subject {
                let (parent, leaf) = path.rsplit_once('/').expect("preflight checked pointer");
                payload.pointer_mut(parent).and_then(Value::as_object_mut).unwrap_or_else(||panic!("preflighted actual optional parent missing: family={family} id={id} path={path}")).insert(leaf.into(),case.replacement.clone());
            } else {
                *payload.pointer_mut(path).unwrap_or_else(|| {
                    panic!("preflighted actual field missing: family={family} id={id} path={path}")
                }) = case.replacement.clone();
            }
            assert_ne!(
                payload,
                serde_json::from_str::<Value>(original_text).unwrap(),
                "actual body mutation must differ: {family}/{id}/{path}"
            );
            change(
                store,
                family,
                id,
                &serde_json::to_string(&payload).unwrap(),
                None,
            );
        }
        assert!(
            store.code_get::<Value>(family, id).is_err(),
            "malformed single {family}/{path} accepted"
        );
        assert!(
            store.code_all::<Value>(family).is_err(),
            "malformed listing {family}/{path} accepted"
        );
        if family == CODE_BINDING {
            assert!(
                store
                    .get_binding(&scope(), &RepoId("repo".into()))
                    .await
                    .is_err()
            );
            assert!(
                store.code_view(false).is_err(),
                "native snapshot accepted corrupt authority"
            );
            let session = store.scope_session().unwrap();
            let bound = session.store();
            let step = bound.begin_step().unwrap();
            assert!(
                bound.code_view(false).is_err(),
                "staged snapshot accepted corrupt authority"
            );
            drop(step);
            drop(bound);
            drop(session);
        }
        // Restore the exact original text, including absent optional fields; no normalization.
        change(
            store,
            family,
            id,
            original_text,
            if case.projection {
                Some((path, &original[path]))
            } else {
                None
            },
        );
        assert_eq!(
            physical(store, family, id)["payload"],
            original["payload"],
            "exact original payload was not restored: {family}/{id}/{path}"
        );
        assert!(
            store.code_get::<Value>(family, id).is_ok(),
            "valid original failed after declared control restoration"
        );
    }
    fn exercise(mode: Mode) {
        block_on(async {
            let now = chrono::Utc::now();
            let expires = now + chrono::Duration::hours(1);
            let cases = corruption_cases(expires);
            assert_preflight(&cases, expires);
            let f = Fixture::new(mode);
            let s = &f.store;
            let repo = RepoId("repo".into());
            s.register(RepositoryRegistration {
                repo_id: repo.clone(),
                locator_id: "fixture-locator".into(),
                acquisition: AcquisitionMode::LocalOnly,
            })
            .await
            .unwrap();
            s.bind(CodeBinding {
                scope: scope(),
                repo_id: repo.clone(),
                branch_ref: "refs/heads/main".into(),
                version: 1,
                active_generation: None,
                epoch: 0,
            })
            .await
            .unwrap();
            ready_code(s, &repo, "g1", "a1", 0, None).await;
            s.publish(&scope(), &repo, "a1", 0).await.unwrap();
            s.put_lineage(&scope(), "first", code_lineage("repo", "g1"))
                .await
                .unwrap();
            let src = IndexedSource {
                repo_id: repo.clone(),
                generation_id: "g1".into(),
                commit_oid: code_generation("g1").commit_oid,
                path: "src/lib.rs".into(),
                content_digest: "3".repeat(64),
            };
            s.put_source_descriptor(&scope(), src).await.unwrap();
            let eligibility = s.eligibility(&scope()).await.unwrap();
            let lease = s
                .acquire_read_lease(
                    CodeReadLease {
                        id: "lease".into(),
                        eligibility: eligibility.clone(),
                        fencing_token: 1,
                        expires_at: expires,
                    },
                    now,
                )
                .await
                .unwrap();
            let auxiliary = s
                .acquire_read_lease(
                    CodeReadLease {
                        id: "auxiliary".into(),
                        eligibility,
                        fencing_token: 1,
                        expires_at: lease.expires_at,
                    },
                    now,
                )
                .await
                .unwrap();
            s.release_read_lease(&auxiliary).await.unwrap();
            s.put_ref_observation(
                &scope(),
                &repo,
                CodeRefObservation {
                    branch_ref: "refs/heads/main".into(),
                    observed_oid: "4".repeat(40),
                    observed_at: now,
                    policy: RefObservationPolicy::OnRequest,
                },
            )
            .await
            .unwrap();
            let preview = s.preview_reset(&scope(), &repo).await.unwrap();
            let journal = CodeResetJournal {
                reset_id: "reset".into(),
                preview_hash: preview.preview_hash.clone(),
                target_fingerprints: preview
                    .targets
                    .iter()
                    .map(|t| (t.record_id.clone(), t.fingerprint.clone()))
                    .collect(),
                authorizing_decisions: vec!["fixture-decision".into()],
                checkpoints: vec![],
                preservation_digests: preview.preservation_digests.clone(),
                preview,
            };
            s.journal_reset(journal).await.unwrap();
            ready_code(s, &repo, "g2", "a2", 1, Some("g1")).await;
            for case in cases.iter().filter(|c| c.phase == Phase::BeforeRetirement) {
                reject(s, case).await;
            }
            s.release_read_lease(&lease).await.unwrap();
            s.retire_lineage_targets_with_reset_progress(
                &scope(),
                &[(repo.clone(), 1)],
                &[RetirementTarget {
                    repo_id: "repo".into(),
                    generation_id: "g1".into(),
                    record_id: "first".into(),
                }],
                Some(("reset", &["first".into()])),
            )
            .await
            .unwrap();
            for case in cases.iter().filter(|c| c.phase == Phase::AfterRetirement) {
                reject(s, case).await;
            }
            assert_eq!(
                s.get_binding(&scope(), &repo).await.unwrap().unwrap().epoch,
                2,
                "retirement-advanced published binding remains valid"
            );
            // Independent staging positive follows reset controls: its deliberately unpublished
            // parent must not contaminate their canonical ownership preview.
            // Legal preparation is not publication; missing future parents do not bar staging.
            s.put_lineage(&scope(), "pre-stage", code_lineage("repo", "future"))
                .await
                .unwrap();
            assert!(
                s.record_lineage(&scope(), "pre-stage")
                    .await
                    .unwrap()
                    .is_some()
            );
            println!(
                "TASK006 {mode:?}: typed code-family malformed-body/projection/immutable-parent controls deny; valid staging and retirement-advanced epochs preserved"
            );
        });
    }
    #[cfg(feature = "embedded")]
    #[test]
    fn embedded_typed_code_family_identity_corruption_is_rejected() {
        exercise(Mode::Embedded)
    }
    #[cfg(feature = "service")]
    #[test]
    fn service_typed_code_family_identity_corruption_is_rejected() {
        exercise(Mode::Service)
    }
}
