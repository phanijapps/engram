//! Surreal backend wiring: construct the embedded SurrealKV-backed cells from
//! configuration and return a wired provider.
//!
//! ADR-0022 (amended 2026-07-16): this module names `Surreal*` / holds the
//! engine adapters by design and is intentionally exempt from the
//! engine-neutrality gate.
//!
//! Connection lifecycle: each cell opens its SurrealKV connection LAZILY on
//! first use (under the consumer's Tokio runtime). The Surreal SDK requires a
//! Tokio reactor, and `bootstrap_surreal` is sync (called from sync
//! `EngramProvider::open`), so the open cannot happen here.

use std::sync::Arc;

use crate::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_domain::CapabilityState;
use engram_memory::MemoryService;
use engram_runtime::CoreResult;
use engram_store_surreal::{SurrealConnection, SurrealMemoryService};

/// Bootstraps a fully-wired provider from configuration against the Surreal
/// backend (embedded SurrealKV).
///
/// v1 memory cell wired (plan T4): the memory capability is `Supported` and
/// round-trips write → retrieve → forget through the facade, mirroring the
/// SQLite lifecycle. Other capability cells (knowledge, belief, hierarchy,
/// vectors, consolidation) wire in subsequent increments; until then they
/// report `ProviderUnavailable` (fail-closed, conformance contract).
pub(crate) fn bootstrap_surreal(config: &EngramConfig) -> CoreResult<EngramProvider> {
    let path = config.storage_path.to_string_lossy().to_string();
    // One shared Surreal connection; every Surreal cell (memory now, knowledge/
    // belief/hierarchy/vectors later) clones this Arc.
    let conn = Arc::new(SurrealConnection::new(path));
    let memory: Arc<dyn MemoryService> = Arc::new(SurrealMemoryService::new(conn));
    let report = CapabilityReport::builder()
        .memory(CapabilityState::Supported)
        .build();
    Ok(EngramProviderBuilder::new(report).memory(memory).build())
}

#[cfg(test)]
mod tests {
    //! Surreal-cell tests — compile + run only with `--features surreal`.
    use super::*;
    use chrono::Utc;
    use engram_domain::{
        types::ScopeMappingStrategy, Actor, ActorKind, AllowedUse, DeleteMode, ForgetStatus,
        ForgetTargetType, ForgetRequest, Id, MemoryContent, MemoryEventKind, MemoryKind,
        MemoryStatus, Policy, Provenance, Requester, Retention, RetrievalRequest, Scope,
        Sensitivity, Visibility, WriteMemoryRequest,
    };
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> EngramConfig {
        EngramConfig::new(
            dir.path().join("surreal"),
            dir.path(),
            ScopeMappingStrategy::Strict,
            crate::EmbeddingProviderConfig {
                provider_type: "fastembed".to_string(),
                model: "m".to_string(),
                dimensions: 384,
                prompt_profile: "query".to_string(),
                normalization: None,
            },
            crate::MigrationMode::DryRun,
            crate::CapabilityPolicy::FailClosed,
        )
    }

    fn scope(tenant: &str) -> Scope {
        Scope {
            tenant: tenant.to_owned(),
            subject: Some("subject-a".to_owned()),
            workspace: Some("workspace-a".to_owned()),
            session: None,
            environment: Some("test".to_owned()),
        }
    }

    fn actor() -> Actor {
        Actor {
            id: Id::from("surreal-agent"),
            kind: ActorKind::Agent,
            display_name: Some("Surreal".to_owned()),
            metadata: None,
        }
    }

    fn requester() -> Requester {
        Requester {
            actor: actor(),
            roles: Vec::new(),
            permissions: vec!["memory.write".to_owned(), "memory.retrieve".to_owned()],
            on_behalf_of: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "surreal-test".to_owned(),
            actor: actor(),
            observed_at: Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("manual".to_owned()),
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Medium),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }

    fn write_request(tenant: &str) -> WriteMemoryRequest {
        WriteMemoryRequest {
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "surreal round-trip memory".to_owned(),
                summary: None,
                entities: Vec::new(),
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope: scope(tenant),
            requester: requester(),
            provenance: provenance(),
            policy: policy(),
            links: Vec::new(),
            idempotency_key: None,
        }
    }

    fn retrieve_request(tenant: &str) -> RetrievalRequest {
        RetrievalRequest {
            query: "surreal".to_owned(),
            scope: scope(tenant),
            requester: requester(),
            modes: Vec::new(),
            filters: None,
            cues: Vec::new(),
            limit: Some(10),
            budget: None,
            include_explanations: None,
        }
    }

    fn forget_request(id: &str, tenant: &str) -> ForgetRequest {
        ForgetRequest {
            target_type: ForgetTargetType::Memory,
            target_id: id.to_owned(),
            scope: scope(tenant),
            requester: requester(),
            mode: DeleteMode::Tombstone,
            reason: None,
        }
    }

    #[test]
    fn bootstrap_surreal_constructs_provider_with_memory_supported() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        assert!(
            provider.capabilities().memory.is_supported(),
            "the Surreal memory cell (T4) must mark memory Supported"
        );
        assert!(
            provider.memory().is_some(),
            "the memory handle must be wired"
        );
    }

    /// T4 proof: the Surreal memory cell round-trips write → retrieve → forget
    /// through the facade's memory handle, with scope isolation — the same
    /// lifecycle the SQLite fixtures + S7 stub exercise, here against SurrealKV.
    #[tokio::test]
    async fn surreal_memory_round_trips_write_retrieve_forget() {
        let dir = TempDir::new().unwrap();
        let provider = bootstrap_surreal(&test_config(&dir)).expect("surreal bootstrap");
        let memory = provider.memory().expect("memory handle wired");

        // write
        let stored = memory
            .write_memory(write_request("tenant-a"))
            .await
            .expect("write_memory against Surreal");
        let memory_id = stored.record.id.to_string();
        assert_eq!(stored.record.status, MemoryStatus::Active);
        assert_eq!(stored.event.kind, MemoryEventKind::Written);

        // retrieve — tenant-a sees it
        let visible = memory
            .retrieve(retrieve_request("tenant-a"))
            .await
            .expect("retrieve tenant-a");
        assert!(
            visible.items.iter().any(|r| r.target_id == memory_id),
            "tenant-a must see its Surreal-stored memory"
        );

        // scope isolation — tenant-b must not
        let hidden = memory
            .retrieve(retrieve_request("tenant-b"))
            .await
            .expect("retrieve tenant-b");
        assert!(
            !hidden.items.iter().any(|r| r.target_id == memory_id),
            "Surreal backend must not leak memories across tenants"
        );

        // forget (tombstone)
        let forgotten = memory
            .forget(forget_request(&memory_id, "tenant-a"))
            .await
            .expect("forget against Surreal");
        assert_eq!(forgotten.status, ForgetStatus::Tombstoned);

        // after tombstone, not visible
        let after = memory
            .retrieve(retrieve_request("tenant-a"))
            .await
            .expect("retrieve after forget");
        assert!(
            !after.items.iter().any(|r| r.target_id == memory_id),
            "tombstoned memory must not appear in Surreal retrieval"
        );
    }
}
