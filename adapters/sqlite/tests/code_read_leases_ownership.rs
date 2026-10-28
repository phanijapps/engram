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
fn anchored_renewal_extends_historical_eligibility_across_drift() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let path = directory.path().join("anchored-renewal.sqlite");
    let repository = SqlCodeBindingRepository::open(&path).expect("open repository");
    let anchored_scope = scope("anchored-drift");
    block_on(repository.bind(binding(&anchored_scope, "first-repository"))).expect("bind first");
    let historical =
        block_on(repository.eligibility(&anchored_scope)).expect("historical eligibility");
    let now = chrono::Utc::now();
    let lease = block_on(repository.acquire_read_lease(
        CodeReadLease {
            id: "anchored-renewal-lease".to_owned(),
            eligibility: historical.clone(),
            fencing_token: 1,
            expires_at: now + chrono::Duration::minutes(10),
        },
        now,
    ))
    .expect("acquire anchored lease");
    // Unrelated scope-eligibility advance after acquisition: a second binding
    // advances the scope epoch without touching the held lease.
    block_on(repository.bind(binding(&anchored_scope, "second-repository"))).expect("bind second");
    let drifted = block_on(repository.eligibility(&anchored_scope)).expect("drifted eligibility");
    assert!(drifted.epoch > historical.epoch);
    let renewed = block_on(repository.renew_read_lease(
        &lease,
        now + chrono::Duration::seconds(1),
        now + chrono::Duration::minutes(20),
    ))
    .expect("renewal extends the anchored historical snapshot across drift");
    assert_eq!(renewed.eligibility, historical);
    assert_eq!(renewed.fencing_token, lease.fencing_token + 1);
    assert!(
        matches!(
            block_on(repository.renew_read_lease(
                &lease,
                now + chrono::Duration::seconds(2),
                now + chrono::Duration::minutes(30),
            )),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ),
        "superseded pre-renewal handle rejects through the stale-fence record comparison"
    );
    assert!(
        matches!(
            block_on(repository.acquire_read_lease(
                CodeReadLease {
                    id: "anchored-stale-acquire".to_owned(),
                    eligibility: historical,
                    fencing_token: 1,
                    expires_at: now + chrono::Duration::minutes(30),
                },
                now + chrono::Duration::seconds(2),
            )),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ),
        "initial acquisition still requires current eligibility"
    );
    assert!(matches!(
        block_on(repository.release_read_lease(&lease)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    block_on(repository.release_read_lease(&renewed)).expect("release renewed lease");
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
