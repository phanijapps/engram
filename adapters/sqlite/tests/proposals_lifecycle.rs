//! Operator proposal queue lifecycle tests (enhancements adoption, slice 2).
//!
//! Acceptance tests from the spec: approval routes through the policy-checked
//! memory write path (a denying authorizer leaves NO active memory), pending
//! and rejected proposals never appear in the memory store, retries are
//! idempotent, and the CAS state machine rejects illegal transitions.

use std::sync::Arc;

use engram_domain::*;

use engram_proposals::{
    AllowAllProposalAuthorizer, DenyAllProposalAuthorizer, ProposalAuthorizer, ProposalRepository,
    ProposalService,
};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use engram_store_sqlite::{SqlMemoryService, SqlProposalStore};
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
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

fn agent(id: &str) -> Actor {
    Actor {
        id: ActorId::from(id),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn human(id: &str) -> Actor {
    Actor {
        id: ActorId::from(id),
        kind: ActorKind::User,
        display_name: Some(id.to_owned()),
        metadata: None,
    }
}

fn submit_request(text: &str) -> ProposalSubmitRequest {
    ProposalSubmitRequest {
        scope: scope(),
        payload: ProposalMemoryWrite {
            kind: MemoryKind::Fact,
            content: MemoryContent {
                text: text.to_owned(),
                summary: None,
                entities: vec![],
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            policy: policy(),
            links: vec![],
        },
        submitted_by: agent("worker-1"),
        submission_key: None,
    }
}

fn service_with_authorizer(
    memory_authorizer: Arc<dyn PolicyAuthorizer>,
    proposal_authorizer: Arc<dyn ProposalAuthorizer>,
) -> (SqlMemoryService, ProposalService) {
    let memory = SqlMemoryService::with_dependencies(
        engram_store_sqlite::SqlMemoryStore::open_in_memory().expect("store"),
        memory_authorizer,
        Arc::new(engram_store_sqlite::SystemClock),
        Arc::new(engram_store_sqlite::SequentialIdGenerator::new()),
    )
    .expect("memory service");
    let repository = Arc::new(SqlProposalStore::open_in_memory().expect("proposals"));
    let service = ProposalService::new(Arc::new(memory.clone()), repository, proposal_authorizer);
    (memory, service)
}

/// Approval must produce exactly one policy-checked memory plus an approved
/// proposal carrying the receipt; retrying the approval is idempotent.
#[test]
fn approve_promotes_through_memory_service_with_receipt_and_idempotent_retry() {
    let (memory, service) = service_with_authorizer(
        Arc::new(engram_store_sqlite::AllowAllPolicyAuthorizer),
        Arc::new(AllowAllProposalAuthorizer),
    );
    let submitted =
        block_on(service.submit(submit_request("Zebra secret: deploy gate holds at v2")))
            .expect("submit");
    assert_eq!(submitted.proposal.status, OperatorProposalStatus::Pending);
    assert!(!submitted.proposal.payload_fingerprint.is_empty());

    // Not promoted yet: memory store is empty.
    let page = block_on(
        <SqlMemoryService as engram_memory::MemoryRepository>::list_memories_paged(
            &memory,
            &scope(),
            None,
            10,
        ),
    )
    .expect("list");
    assert!(
        page.items.is_empty(),
        "pending proposal must not be a memory"
    );

    let approved = block_on(service.approve(ProposalApproveRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: submitted.proposal.version,
        reviewer: human("operator-1"),
    }))
    .expect("approve");
    assert_eq!(approved.proposal.status, OperatorProposalStatus::Approved);
    let memory_id = approved.memory_id.expect("promotion receipt");

    // Exactly one memory, matching the payload, written with the
    // deterministic per-proposal idempotency key.
    let page = block_on(
        <SqlMemoryService as engram_memory::MemoryRepository>::list_memories_paged(
            &memory,
            &scope(),
            None,
            10,
        ),
    )
    .expect("list");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, memory_id);
    assert!(page.items[0].content.text.contains("deploy gate"));

    // Idempotent retry: same memory, no second record.
    let retried = block_on(service.approve(ProposalApproveRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: approved.proposal.version,
        reviewer: human("operator-1"),
    }))
    .expect("retry approve");
    assert!(retried.deduplicated);
    assert_eq!(retried.memory_id, Some(memory_id));
    let page = block_on(
        <SqlMemoryService as engram_memory::MemoryRepository>::list_memories_paged(
            &memory,
            &scope(),
            None,
            10,
        ),
    )
    .expect("list");
    assert_eq!(page.items.len(), 1, "retry must not promote twice");
}

/// A denying MEMORY authorizer blocks the promotion: the error surfaces and
/// no active memory exists — the queue never bypasses policy.
#[test]
fn approval_denied_by_memory_policy_leaves_no_active_memory() {
    #[derive(Debug, Clone, Copy, Default)]
    struct DenyAllMemory;

    impl PolicyAuthorizer for DenyAllMemory {
        fn can_write(
            &self,
            _requester: &Requester,
            _scope: &Scope,
            _policy: &Policy,
        ) -> CoreResult<()> {
            Err(CoreError::PolicyDenied {
                reason: "test denial".to_owned(),
            })
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

    let (memory, service) = service_with_authorizer(
        Arc::new(DenyAllMemory),
        Arc::new(AllowAllProposalAuthorizer),
    );
    let submitted = block_on(service.submit(submit_request("Quokka secret"))).expect("submit");
    let err = block_on(service.approve(ProposalApproveRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: submitted.proposal.version,
        reviewer: human("operator-1"),
    }))
    .expect_err("denied write must fail");
    assert!(matches!(err, CoreError::PolicyDenied { .. }));

    let page = block_on(
        <SqlMemoryService as engram_memory::MemoryRepository>::list_memories_paged(
            &memory,
            &scope(),
            None,
            10,
        ),
    )
    .expect("list");
    assert!(
        page.items.is_empty(),
        "policy denial must leave no active memory"
    );

    // The proposal stays applying (claim held, nothing committed) — a retry
    // would re-run the same idempotent path; rejection cannot overtake it.
    let listed = block_on(service.list(ProposalListRequest {
        scope: scope(),
        status: None,
        after: None,
        limit: 10,
    }))
    .expect("list");
    assert_eq!(
        listed.items[0].status,
        OperatorProposalStatus::Applying,
        "failed approval leaves the visible in-progress state"
    );
    let rejection = block_on(service.reject(ProposalRejectRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: listed.items[0].version,
        reviewer: human("operator-1"),
        reason: Some("deny".into()),
    }));
    assert!(
        rejection.is_err(),
        "rejection cannot overtake a possibly-committed approval"
    );
}

/// Rejected proposals never promote; resubmission is the only edit path.
#[test]
fn reject_keeps_payload_inert_and_never_writes_memory() {
    let (memory, service) = service_with_authorizer(
        Arc::new(engram_store_sqlite::AllowAllPolicyAuthorizer),
        Arc::new(AllowAllProposalAuthorizer),
    );
    let submitted = block_on(service.submit(submit_request("Wombat secret"))).expect("submit");
    let rejected = block_on(service.reject(ProposalRejectRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: submitted.proposal.version,
        reviewer: human("operator-1"),
        reason: Some("not now".into()),
    }))
    .expect("reject");
    assert_eq!(rejected.proposal.status, OperatorProposalStatus::Rejected);
    assert!(rejected.proposal.decision.is_some());

    let page = block_on(
        <SqlMemoryService as engram_memory::MemoryRepository>::list_memories_paged(
            &memory,
            &scope(),
            None,
            10,
        ),
    )
    .expect("list");
    assert!(page.items.is_empty(), "rejected proposal must stay inert");

    // Approving a rejected proposal is an explicit conflict.
    let err = block_on(service.approve(ProposalApproveRequest {
        id: submitted.proposal.id.clone(),
        scope: scope(),
        expected_version: rejected.proposal.version,
        reviewer: human("operator-1"),
    }))
    .expect_err("rejected proposals cannot be approved");
    assert!(matches!(err, CoreError::Conflict { .. }));
}

/// The proposal authorizer gates submission and review: a denied submitter
/// never reaches the queue; a denied reviewer never sees it.
#[test]
fn proposal_authorizer_gates_submit_and_approve() {
    let (_memory, service) = service_with_authorizer(
        Arc::new(engram_store_sqlite::AllowAllPolicyAuthorizer),
        Arc::new(DenyAllProposalAuthorizer),
    );
    let err = block_on(service.submit(submit_request("Potoroo secret"))).expect_err("denied");
    assert!(matches!(err, CoreError::PolicyDenied { .. }));

    // An actor-supplied "operator" identity in a payload is not authority:
    // approve with any reviewer still fails the authorizer.
    let err = block_on(service.approve(ProposalApproveRequest {
        id: OperatorProposalId::from("proposal-does-not-matter"),
        scope: scope(),
        expected_version: 1,
        reviewer: human("operator-1"),
    }))
    .expect_err("denied reviewer");
    assert!(matches!(err, CoreError::PolicyDenied { .. }));
}

/// Repository CAS: exactly one claimant moves pending → applying; version
/// mismatches conflict; scope isolation hides foreign proposals.
#[test]
fn repository_cas_and_scope_isolation() {
    let repo = SqlProposalStore::open_in_memory().expect("repo");
    let now = chrono::Utc::now();
    let proposal = OperatorProposal {
        id: OperatorProposalId::from("proposal-cas-1"),
        scope: scope(),
        payload: ProposalMemoryWrite {
            kind: MemoryKind::Fact,
            content: MemoryContent {
                text: "Numbat secret".into(),
                summary: None,
                entities: vec![],
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            policy: policy(),
            links: vec![],
        },
        status: OperatorProposalStatus::Pending,
        submitted_by: agent("worker-1"),
        payload_fingerprint: "fp".into(),
        version: 1,
        created_at: now,
        updated_at: None,
        decision: None,
        promotion: None,
    };
    let stored = block_on(repo.insert_proposal(proposal.clone())).expect("insert");

    // First claim wins; second conflicts.
    let claimed =
        block_on(repo.claim_approval(&stored.id, &scope(), stored.version)).expect("claim 1");
    assert_eq!(claimed.status, OperatorProposalStatus::Applying);
    assert_eq!(claimed.version, stored.version + 1);
    let second = block_on(repo.claim_approval(&stored.id, &scope(), stored.version));
    assert!(matches!(second, Err(CoreError::Conflict { .. })));

    // Wrong-version reject conflicts.
    let bad = block_on(repo.reject_proposal(
        &stored.id,
        &scope(),
        stored.version,
        ProposalDecision {
            reviewer: human("op"),
            decided_at: chrono::Utc::now(),
            reason: None,
        },
    ));
    assert!(matches!(bad, Err(CoreError::Conflict { .. })));

    // Foreign scope: absent, not leaked.
    let mut foreign = scope();
    foreign.tenant = "tenant-b".into();
    let got = block_on(repo.get_proposal(&stored.id, &foreign)).expect("get");
    assert!(got.is_none(), "cross-scope proposal read must be absent");
}
