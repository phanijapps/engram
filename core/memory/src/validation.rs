//! Engine-neutral request validation for memory writes.
//!
//! The single shared seam every `MemoryService` write path rides (FIND-code-101):
//! SQLite and Postgres/pgvector adapters call [`validate_write_request`] so
//! one definition of the v1 write rules covers every write surface — the
//! MCP tools, the Rust facade, the N-API binding, and each backend recipe.
//! This crate holds no engine types; the checks operate on domain contracts
//! only (ADR-0022 rule-1 clean).

use engram_domain::{AllowedUse, Verdict, WriteMemoryRequest};
use engram_runtime::{CoreError, CoreResult};

/// Validates behavior-level constraints for v1 memory writes.
///
/// Catches service rules deserialization alone cannot express: required
/// scope/content/provenance fields, the v1 training-export ban, and the
/// LED-14 reservation — `derived` is rejected as input with the
/// reserved-for-derivation error on every write surface.
pub fn validate_write_request(request: &WriteMemoryRequest) -> CoreResult<()> {
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
    use chrono::Utc;
    use engram_domain::{
        Actor, ActorId, ActorKind, MemoryContent, MemoryKind, Policy, Provenance, Requester,
        Retention, Scope, Visibility, WriteMemoryRequest,
    };

    fn base_request() -> WriteMemoryRequest {
        WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "x".to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: Some("w".to_owned()),
                session: None,
                environment: None,
            },
            requester: Requester {
                actor: Actor {
                    id: ActorId::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            provenance: Provenance {
                source: "s".to_owned(),
                actor: Actor {
                    id: ActorId::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
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

    /// FIND-code-101 seam guard: the shared validation the pgvector write
    /// path now rides rejects the reserved word and accepts the two
    /// writable ones — one definition, every backend.
    #[test]
    fn shared_validation_rejects_reserved_and_accepts_writable_verdicts() {
        let mut request = base_request();
        request.verdict = Some(Verdict::Derived);
        let err = validate_write_request(&request).expect_err("derived must be rejected");
        assert!(
            err.to_string().contains("reserved"),
            "reserved-for-derivation reason: {err}"
        );
        request.verdict = Some(Verdict::AssertedByAgent);
        assert!(validate_write_request(&request).is_ok());
        request.verdict = Some(Verdict::GroundedInSource);
        assert!(validate_write_request(&request).is_ok());
        request.verdict = None;
        assert!(validate_write_request(&request).is_ok());
    }

    #[test]
    fn shared_validation_keeps_the_v1_field_rules() {
        let mut request = base_request();
        request.content.text = "  ".to_owned();
        assert!(validate_write_request(&request).is_err());
        let mut request = base_request();
        request.policy.allowed_uses.push(AllowedUse::TrainingExport);
        assert!(validate_write_request(&request).is_err());
    }
}
