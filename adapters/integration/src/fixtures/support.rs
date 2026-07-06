//! Shared domain constructors for conformance fixtures.
//!
//! Fixtures build minimal but real domain objects so each capability is
//! exercised against the actual adapter, not a stub.

use chrono::Utc;
use engram_domain::*;

pub(crate) fn actor() -> Actor {
    Actor {
        id: Id::from("conformance-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Conformance Harness".to_owned()),
        metadata: None,
    }
}

pub(crate) fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

pub(crate) fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Medium),
        allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Evaluation],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

pub(crate) fn provenance() -> Provenance {
    Provenance {
        source: "conformance".to_owned(),
        actor: actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}
