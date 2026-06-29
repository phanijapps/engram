use std::sync::Arc;

use chrono::{TimeZone, Utc};
use engram_core::{Clock, CoreError, CoreResult, MemoryService, PolicyAuthorizer};
use engram_domain::*;
use engram_store_memory::{InMemoryMemoryService, SequentialIdGenerator};
use futures::executor::block_on;

#[derive(Debug)]
struct FixedClock(Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Debug)]
struct AllowAll;

impl PolicyAuthorizer for AllowAll {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct DenyRetrieve;

impl PolicyAuthorizer for DenyRetrieve {
    fn can_write(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn can_retrieve(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "test retrieve denial".to_owned(),
        })
    }

    fn can_forget(
        &self,
        _requester: &Requester,
        _scope: &Scope,
        _policy: &Policy,
    ) -> CoreResult<()> {
        Ok(())
    }
}

fn fixed_time() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0)
        .single()
        .expect("fixed timestamp")
}

fn actor() -> Actor {
    Actor {
        id: Id::from("actor-agent-1"),
        kind: ActorKind::Agent,
        display_name: Some("Retrieve Agent".to_owned()),
        metadata: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: actor(),
        roles: vec!["maintainer".to_owned()],
        permissions: vec!["memory.retrieve".to_owned()],
        on_behalf_of: None,
    }
}

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_owned(),
        subject: None,
        workspace: Some("engram".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy(allowed_uses: Vec<AllowedUse>) -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses,
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "retrieve_context_spec".to_owned(),
        actor: actor(),
        observed_at: fixed_time(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_owned()),
    }
}

fn write_request(text: &str, tenant: &str, allowed_uses: Vec<AllowedUse>) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.to_owned(),
            summary: None,
            entities: Vec::new(),
            language: Some("en".to_owned()),
            format: Some(MemoryContentFormat::Text),
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        requester: requester(),
        provenance: provenance(),
        policy: policy(allowed_uses),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn retrieval_request(query: &str, tenant: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.to_owned(),
        scope: scope(tenant),
        requester: requester(),
        modes: vec![RetrievalMode::Keyword],
        filters: Some(QueryFilter {
            memory_kinds: vec![MemoryKind::Fact],
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            min_confidence: None,
            include_archived: Some(false),
        }),
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: Some(true),
    }
}

fn service(authorizer: Arc<dyn PolicyAuthorizer>) -> InMemoryMemoryService {
    InMemoryMemoryService::with_dependencies(
        authorizer,
        Arc::new(FixedClock(fixed_time())),
        Arc::new(SequentialIdGenerator::new()),
    )
}

#[test]
fn retrieve_returns_keyword_match_with_explanation() {
    let service = service(Arc::new(AllowAll));
    block_on(service.write_memory(write_request(
        "Engram uses Rust 2024 for the deterministic core and TypeScript bindings.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write matching memory");
    block_on(service.write_memory(write_request(
        "Unrelated memory about release packaging.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write unrelated memory");

    let context =
        block_on(service.retrieve(retrieval_request("Rust TypeScript bindings", "tenant-demo")))
            .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.items[0].target_type, RetrievalTargetType::Memory);
    assert!(context.items[0].content.contains("Rust 2024"));
    assert_eq!(context.items[0].score.policy_fit, Some(1.0));
    let explanation = context.items[0]
        .explanation
        .as_ref()
        .expect("retrieval explanation");
    assert_eq!(
        explanation.matched_terms,
        vec!["bindings", "rust", "typescript"]
    );
    assert!(context.omitted.is_empty());
    assert_eq!(context.created_at, fixed_time());
}

#[test]
fn retrieve_does_not_cross_tenant_boundary() {
    let service = service(Arc::new(AllowAll));
    block_on(service.write_memory(write_request(
        "Tenant demo memory about Rust bindings.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write tenant demo memory");
    block_on(service.write_memory(write_request(
        "Tenant other memory about Rust bindings.",
        "tenant-other",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write tenant other memory");

    let context = block_on(service.retrieve(retrieval_request("Rust bindings", "tenant-demo")))
        .expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert!(context.items[0].content.contains("Tenant demo"));
}

#[test]
fn retrieve_omits_policy_denied_candidate() {
    let service = service(Arc::new(DenyRetrieve));
    block_on(service.write_memory(write_request(
        "Denied memory about Rust bindings.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write denied memory");

    let context = block_on(service.retrieve(retrieval_request("Rust bindings", "tenant-demo")))
        .expect("retrieve context");

    assert!(context.items.is_empty());
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::PolicyDenied);
}

#[test]
fn retrieve_reports_budget_exceeded_candidates() {
    let service = service(Arc::new(AllowAll));
    block_on(service.write_memory(write_request(
        "First Rust bindings memory.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write first memory");
    block_on(service.write_memory(write_request(
        "Second Rust bindings memory.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write second memory");
    let mut request = retrieval_request("Rust bindings", "tenant-demo");
    request.budget = Some(ContextBudget {
        max_items: Some(1),
        max_tokens: None,
        max_bytes: None,
    });

    let context = block_on(service.retrieve(request)).expect("retrieve context");

    assert_eq!(context.items.len(), 1);
    assert_eq!(context.omitted.len(), 1);
    assert_eq!(context.omitted[0].reason, OmittedReason::BudgetExceeded);
}

#[test]
fn retrieve_returns_empty_context_for_no_result_query() {
    let service = service(Arc::new(AllowAll));
    block_on(service.write_memory(write_request(
        "Memory about Rust bindings.",
        "tenant-demo",
        vec![AllowedUse::Retrieval],
    )))
    .expect("write memory");

    let context =
        block_on(service.retrieve(retrieval_request("unmatched banana query", "tenant-demo")))
            .expect("retrieve context");

    assert!(context.items.is_empty());
    assert!(context.omitted.is_empty());
    assert!(context.source_failures.is_empty());
}
