use engram_domain::{CodeBinding, RepoId, Scope};
use engram_knowledge::{CodeBindingRepository, CodeReadLease, CodeReadLeases, CoreError};
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;

fn scope(workspace: &str) -> Scope {
    Scope {
        tenant: "lease-ownership-test".into(),
        subject: None,
        workspace: Some(workspace.into()),
        session: None,
        environment: None,
    }
}

fn binding(scope: &Scope, repo: &str) -> CodeBinding {
    CodeBinding {
        scope: scope.clone(),
        repo_id: RepoId(repo.into()),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }
}

#[test]
fn released_lease_id_is_durably_rejected_across_reopen() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let path = directory.path().join("lease-ownership.sqlite");
    let first_scope = scope("first");
    let second_scope = scope("second");
    let lease_id = "released-lease-id".to_owned();
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(5);
    let first_lease = {
        let repository = SqlCodeBindingRepository::open(&path).expect("open first repository");
        block_on(repository.bind(binding(&first_scope, "first-repository"))).expect("bind first");
        let lease = CodeReadLease {
            id: lease_id.clone(),
            eligibility: block_on(repository.eligibility(&first_scope)).expect("first eligibility"),
            fencing_token: 1,
            expires_at,
        };
        let lease = block_on(repository.acquire_read_lease(lease, now)).expect("acquire first");
        block_on(repository.release_read_lease(&lease)).expect("release first");
        lease
    };

    let repository = SqlCodeBindingRepository::open(&path).expect("reopen repository");
    block_on(repository.bind(binding(&second_scope, "second-repository"))).expect("bind second");
    let replacement_request = CodeReadLease {
        id: lease_id,
        eligibility: block_on(repository.eligibility(&second_scope)).expect("second eligibility"),
        fencing_token: 1,
        expires_at: expires_at + chrono::Duration::minutes(1),
    };
    assert!(matches!(
        block_on(repository.acquire_read_lease(replacement_request, now)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert!(matches!(
        block_on(repository.release_read_lease(&first_lease)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert!(matches!(
        block_on(repository.renew_read_lease(
            &first_lease,
            now,
            expires_at + chrono::Duration::minutes(2),
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
}
