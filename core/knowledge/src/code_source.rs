//! Knowledge-owned exact source access and generation fencing ports.
//!
//! Generation changes are local to a knowledge cell. Index projections may
//! lag; consumers must filter their record contributions through this port.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Durable per-file source state. Unsupported cells expose no handle, rather
/// than returning an empty successful source read.
#[async_trait]
pub trait CodeSourceRepository: Send + Sync {
    async fn get_code_source(&self, file: &CodeFileKey)
    -> CoreResult<Option<CodeSourceGeneration>>;

    /// Indexed source/document revision when current and retrieval-authorized.
    /// This metadata lookup never confers a raw-source read grant.
    async fn indexed_code_revision(
        &self,
        _file: &CodeFileKey,
        _scope: &Scope,
        _requester: &Requester,
    ) -> CoreResult<Option<String>> {
        Ok(None)
    }

    /// Compare-and-set to an updating attempt, before destructive replacement.
    async fn begin_code_source(
        &self,
        manifest: CodeSourceGeneration,
        expected_attempt: Option<&str>,
    ) -> CoreResult<()>;

    /// Register contributions before storing the records they guard.
    async fn stage_code_records(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        records: &[CodeSourceRecord],
    ) -> CoreResult<()>;

    /// Atomically checks the updating attempt and staged record before writing.
    async fn write_code_record(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        payload: crate::CodeSourceWrite,
    ) -> CoreResult<()>;

    /// Validates required inventory/snapshot coverage and fences activation.
    async fn activate_code_source(&self, manifest: CodeSourceGeneration) -> CoreResult<()>;

    async fn fail_code_source(&self, file: &CodeFileKey, attempt: &str) -> CoreResult<()>;

    /// Enumerates superseded contributions for cleanup under the current lease.
    async fn prior_code_records(
        &self,
        file: &CodeFileKey,
        attempt: &str,
    ) -> CoreResult<Vec<(String, CodeSourceRecord)>>;

    /// Removes one superseded attempt's ownership under the current updating
    /// lease. Independent active owners and current staged owners preserve it.
    async fn remove_code_record(
        &self,
        file: &CodeFileKey,
        attempt: &str,
        superseded_attempt: &str,
        record: &CodeSourceRecord,
    ) -> CoreResult<bool>;

    async fn list_code_sources(&self, scope: &Scope) -> CoreResult<Vec<CodeSourceGeneration>>;

    /// Atomic repository-root GC checks physical graphs, including inactive ones.
    async fn prune_repository_context(
        &self,
        scope: &Scope,
        repository: &str,
        id: &EntityId,
    ) -> CoreResult<bool> {
        let _ = (scope, repository, id);
        Err(CoreError::CapabilityUnsupported {
            capability: "code_source_context".into(),
            reason: "repository context GC unsupported".into(),
        })
    }
    /// Repository-owned context stays outside file contribution lifecycles.
    async fn ensure_repository_context(
        &self,
        file: &CodeFileKey,
        entity: KnowledgeEntity,
    ) -> CoreResult<()> {
        let _ = (file, entity);
        Err(CoreError::CapabilityUnsupported {
            capability: "code_source_context".into(),
            reason: "source context ensure unsupported".into(),
        })
    }
    /// Atomically merges a repository's shared API projection, retaining prior
    /// semantic states. This source-only operation does not alter KG CRUD's
    /// ordinary hard-delete lifetime. Added source refs must name this repository.
    async fn put_repository_contract(
        &self,
        _scope: &Scope,
        _repository: &str,
        _entity: KnowledgeEntity,
        _relationship: KnowledgeRelationship,
    ) -> CoreResult<()> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code_source_context".into(),
            reason: "repository contract projection unsupported".into(),
        })
    }
    /// Retracts only this repository's edge/ref; shared API nodes survive.
    async fn retract_repository_contract(
        &self,
        _scope: &Scope,
        _repository: &str,
        _entity: &EntityId,
        _relationship: &RelationshipId,
    ) -> CoreResult<()> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code_source_context".into(),
            reason: "repository contract retraction unsupported".into(),
        })
    }
    /// Explicit export requires its own grant, distinct from source read.
    async fn export_code_source(
        &self,
        request: CodeSourceReadRequest,
        authorized_scope: &Scope,
    ) -> CoreResult<CodeSourceBundle> {
        let _ = (request, authorized_scope);
        Err(CoreError::CapabilityUnsupported {
            capability: "code_source_export".into(),
            reason: "source cell does not support export".into(),
        })
    }

    /// `authorized_scope` is resolved by the trusted host, never copied from
    /// the wire request. Implementations must compare it before lookup.
    async fn read_code_source(
        &self,
        request: CodeSourceReadRequest,
        authorized_scope: &Scope,
    ) -> CoreResult<CodeSourceReadResult>;
}

/// Portable source-read policy, deliberately independent of wildcard query
/// scope matching and of any model-processing grant.
pub fn authorize_code_source_scope(
    request: &CodeSourceReadRequest,
    authorized_scope: &Scope,
) -> CoreResult<()> {
    if !request.file.is_valid() || request.file.scope != *authorized_scope {
        return Err(CoreError::PolicyDenied {
            reason: "source scope does not match trusted host scope".into(),
        });
    }
    Ok(())
}

pub fn authorize_code_source_read(
    request: &CodeSourceReadRequest,
    authorized_scope: &Scope,
    manifest: &CodeSourceGeneration,
    now: Timestamp,
) -> CoreResult<()> {
    authorize_code_source_scope(request, authorized_scope)?;
    if request.file != manifest.file {
        return Err(CoreError::PolicyDenied {
            reason: "source scope or file does not match".into(),
        });
    }
    if manifest.state != CodeSourceState::Active
        || request
            .generation
            .as_ref()
            .is_some_and(|generation| generation != &manifest.generation)
    {
        return Err(CoreError::ProviderUnavailable {
            provider: "requested code source generation is unavailable".into(),
        });
    }
    authorize_code_source_access(request, authorized_scope, manifest, now)
}

/// Authorizes source metadata as well as bytes, independently of availability.
pub fn authorize_code_source_access(
    request: &CodeSourceReadRequest,
    authorized_scope: &Scope,
    manifest: &CodeSourceGeneration,
    now: Timestamp,
) -> CoreResult<()> {
    authorize_code_source_scope(request, authorized_scope)?;
    if request.file != manifest.file {
        return Err(CoreError::PolicyDenied {
            reason: "source file does not match".into(),
        });
    }
    if !manifest
        .policy
        .allowed_uses
        .contains(&AllowedUse::Retrieval)
        || manifest
            .policy
            .expires_at
            .is_some_and(|expires| expires <= now)
    {
        return Err(CoreError::PolicyDenied {
            reason: "source retrieval is not permitted".into(),
        });
    }
    let owner = request.requester.actor.id == manifest.provenance.actor.id;
    if !owner
        && (manifest.policy.visibility == Visibility::Private
            || !request
                .requester
                .permissions
                .iter()
                .any(|p| p == "code:source:read"))
    {
        return Err(CoreError::PolicyDenied {
            reason: "source-read authority required".into(),
        });
    }
    if request.max_bytes == 0 || manifest.byte_length > request.max_bytes {
        return Err(CoreError::InvalidRequest {
            reason: "source exceeds requested read limit".into(),
        });
    }
    Ok(())
}

/// Source policy for semantic retrieval, without a raw-read or model grant.
/// Availability is checked separately so authorized failure metadata can be shown.
pub fn code_source_retrieval_authorized(
    manifest: &CodeSourceGeneration,
    requester: &Requester,
    scope: &Scope,
    now: Timestamp,
) -> bool {
    manifest.file.is_valid()
        && manifest.file.scope == *scope
        && source_retrieval_policy_authorized(
            &manifest.policy,
            &manifest.provenance.actor,
            requester,
            now,
        )
}

/// Parent anchors must resolve to a strictly containing declaration in the
/// same file generation. Strict containment also rules out ancestry cycles.
pub fn validate_code_unit_hierarchy<'a>(
    descriptors: impl IntoIterator<Item = &'a CodeChunkDescriptor>,
) -> CoreResult<()> {
    let descriptors: Vec<_> = descriptors.into_iter().collect();
    let mut units = std::collections::HashMap::new();
    let invalid = || CoreError::ValidationFailed {
        reason: "code unit hierarchy is inconsistent".into(),
    };
    for code in &descriptors {
        if let Some(unit) = &code.unit {
            if unit.is_empty() || units.insert(unit.as_str(), *code).is_some() {
                return Err(invalid());
            }
        }
    }
    for code in descriptors {
        if let Some(parent) = &code.parent_unit {
            let parent = units.get(parent.as_str()).ok_or_else(invalid)?;
            let (Some(outer), Some(inner)) = (parent.declaration_range, code.declaration_range)
            else {
                return Err(invalid());
            };
            if parent.file != code.file
                || parent.generation != code.generation
                || outer == inner
                || outer.start > inner.start
                || outer.end < inner.end
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

/// Shared source/document retrieval policy check, independent of raw-read grants.
pub fn source_retrieval_policy_authorized(
    policy: &Policy,
    owner: &Actor,
    requester: &Requester,
    now: Timestamp,
) -> bool {
    policy.allowed_uses.contains(&AllowedUse::Retrieval)
        && policy.expires_at.is_none_or(|expiry| expiry > now)
        && (policy.visibility != Visibility::Private || requester.actor.id == owner.id)
}
