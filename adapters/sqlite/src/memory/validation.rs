//! Request validation for SQL service operations.
//!
//! These checks mirror the in-memory baseline so adapter conformance does not
//! depend on which persistence backend receives the request.

use engram_domain::{AllowedUse, ForgetRequest, RetrievalRequest, Verdict, WriteMemoryRequest};
use engram_memory::{CoreError, CoreResult};

/// Validates behavior-level constraints for v1 memory writes.
///
/// These checks match the in-memory baseline and catch service rules that
/// deserialization alone cannot express, including the v1 training-export ban.
pub(crate) fn validate_write_request(request: &WriteMemoryRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.content.text.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "content.text is required".to_owned(),
        });
    }
    if request.provenance.source.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "provenance.source is required".to_owned(),
        });
    }
    if request
        .policy
        .allowed_uses
        .contains(&AllowedUse::TrainingExport)
    {
        return Err(CoreError::InvalidRequest {
            reason: "policy.allowedUses must not include training_export in v1".to_owned(),
        });
    }
    // FIND-code-002 / FIND-platform-002: the reserved verdict word is
    // rejected at this ONE shared seam so every write surface (facade
    // write_memory, N-API writeMemoryJson, MCP write_memory/store_knowledge)
    // inherits the same closed-set rule the v1 contract documents.
    if request.verdict.is_some_and(Verdict::is_reserved) {
        return Err(CoreError::InvalidRequest {
            reason: "'derived' is reserved for the derivation layer (LED-14): write surfaces \
                    accept only 'asserted_by_agent' or 'grounded_in_source'"
                .to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> WriteMemoryRequest {
        WriteMemoryRequest {
            kind: engram_domain::MemoryKind::Observation,
            content: engram_domain::MemoryContent {
                text: "x".to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: engram_domain::Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: Some("w".to_owned()),
                session: None,
                environment: None,
            },
            requester: engram_domain::Requester {
                actor: engram_domain::Actor {
                    id: engram_domain::ActorId::from("a"),
                    kind: engram_domain::ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            provenance: engram_domain::Provenance {
                source: "s".to_owned(),
                actor: engram_domain::Actor {
                    id: engram_domain::ActorId::from("a"),
                    kind: engram_domain::ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            policy: engram_domain::Policy {
                visibility: engram_domain::Visibility::Workspace,
                retention: engram_domain::Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![],
                expires_at: None,
                delete_mode: None,
            },
            links: Vec::new(),
            verdict: None,
            idempotency_key: None,
        }
    }

    /// FIND-code-002 regression: the reserved word is rejected at the shared
    /// write seam — the check every non-MCP surface inherits.
    #[test]
    fn validate_write_request_rejects_reserved_derived_verdict() {
        let mut request = base_request();
        request.verdict = Some(Verdict::Derived);
        let err = validate_write_request(&request).expect_err("derived must be rejected");
        assert!(
            err.to_string().contains("reserved"),
            "error carries the reserved-for-derivation reason: {err}"
        );
    }

    #[test]
    fn validate_write_request_accepts_the_two_writable_verdicts() {
        let mut request = base_request();
        request.verdict = Some(Verdict::AssertedByAgent);
        assert!(validate_write_request(&request).is_ok());
        request.verdict = Some(Verdict::GroundedInSource);
        assert!(validate_write_request(&request).is_ok());
        request.verdict = None;
        assert!(validate_write_request(&request).is_ok());
    }
}

/// Validates behavior-level constraints for retrieval requests.
///
/// The SQL baseline rejects empty queries and zero budgets before touching
/// storage so adapters share the same caller-facing failure behavior.
pub(crate) fn validate_retrieval_request(request: &RetrievalRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.query.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "query is required".to_owned(),
        });
    }
    if request.limit == Some(0) {
        return Err(CoreError::InvalidRequest {
            reason: "limit must be positive when supplied".to_owned(),
        });
    }
    if let Some(budget) = &request.budget
        && (budget.max_items == Some(0)
            || budget.max_tokens == Some(0)
            || budget.max_bytes == Some(0))
    {
        return Err(CoreError::InvalidRequest {
            reason: "budget limits must be positive when supplied".to_owned(),
        });
    }
    Ok(())
}

/// Validates behavior-level constraints for forget requests.
///
/// Forget validation happens before authorization or mutation to avoid partial
/// lifecycle side effects from malformed target or scope data.
pub(crate) fn validate_forget_request(request: &ForgetRequest) -> CoreResult<()> {
    if request.scope.tenant.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "scope.tenant is required".to_owned(),
        });
    }
    if request.target_id.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "targetId is required".to_owned(),
        });
    }
    Ok(())
}
