//! Outcome-ledger tests (F2): append + windowed read + scope isolation.

use engram_domain::*;
use engram_retrieval::OutcomeLedger;
use engram_store_sqlite::SqlOutcomeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-outcomes".to_owned(),
        subject: None,
        workspace: Some("w".to_owned()),
        session: None,
        environment: None,
    }
}

fn outcome(query_id: &str, success: bool) -> ReportOutcomeRequest {
    ReportOutcomeRequest {
        outcome: RecallOutcome {
            query_id: query_id.to_owned(),
            scope: scope(),
            recalled_targets: vec!["memory-1".to_owned()],
            omitted_targets: vec![],
            config_version: Some("fusion-v1".to_owned()),
            outcome: TaskOutcome {
                success,
                capability: Some("deploy-gate".to_owned()),
                verifier: Actor {
                    id: ActorId::from("host-checker"),
                    kind: ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
            },
            occurred_at: chrono::Utc::now(),
        },
    }
}

#[test]
fn outcomes_append_and_windowed_read_with_scope_isolation() {
    let store = SqlOutcomeStore::open_in_memory().expect("store");
    let first = block_on(store.report_outcome(outcome("q-1", true))).expect("report 1");
    assert!(first.recorded.id.to_string().starts_with("outcome-"));
    let _second = block_on(store.report_outcome(outcome("q-2", false))).expect("report 2");

    let window = OutcomeWindow {
        scope: scope(),
        since: chrono::Utc::now() - chrono::Duration::hours(1),
        until: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    let listed = block_on(store.list_outcomes(&window)).expect("list");
    assert_eq!(listed.len(), 2);
    // Most recent first.
    assert_eq!(listed[0].outcome.query_id, "q-2");
    assert!(
        listed
            .iter()
            .all(|o| o.outcome.outcome.capability.as_deref() == Some("deploy-gate"))
    );

    // Foreign tenant sees nothing.
    let mut foreign = window.clone();
    foreign.scope.tenant = "other-tenant".to_owned();
    let empty = block_on(store.list_outcomes(&foreign)).expect("foreign list");
    assert!(empty.is_empty(), "scope isolation");
}

#[test]
fn session_and_environment_isolate_reads() {
    let store = SqlOutcomeStore::open_in_memory().expect("store");
    let mut scoped = outcome("q-session", true);
    scoped.outcome.scope.session = Some("s1".to_owned());
    scoped.outcome.scope.environment = Some("production".to_owned());
    block_on(store.report_outcome(scoped)).expect("report");

    // Same tenant/workspace, DIFFERENT session -> invisible.
    let mut other_session = OutcomeWindow {
        scope: scope(),
        since: chrono::Utc::now() - chrono::Duration::hours(1),
        until: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    other_session.scope.session = Some("s2".to_owned());
    assert!(
        block_on(store.list_outcomes(&other_session))
            .expect("list")
            .is_empty(),
        "session must isolate"
    );

    // Different environment -> invisible.
    let mut other_env = OutcomeWindow {
        scope: scope(),
        since: chrono::Utc::now() - chrono::Duration::hours(1),
        until: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    other_env.scope.environment = Some("development".to_owned());
    assert!(
        block_on(store.list_outcomes(&other_env))
            .expect("list")
            .is_empty(),
        "environment must isolate"
    );
}

/// Review P1-3 regression: the 500-row limit must apply AFTER scope
/// filtering — 500 newer sibling-workspace rows must not erase this
/// workspace's older history.
#[test]
fn row_limit_applies_after_scope_filtering() {
    let store = SqlOutcomeStore::open_in_memory().expect("store");
    let mut mine = outcome("q-mine", true);
    mine.outcome.scope.workspace = Some("quiet".to_owned());
    block_on(store.report_outcome(mine)).expect("write quiet");
    for i in 0..600 {
        let mut noisy = outcome(&format!("q-noisy-{i}"), true);
        noisy.outcome.scope.workspace = Some("loud".to_owned());
        block_on(store.report_outcome(noisy)).expect("write loud");
    }
    let window = OutcomeWindow {
        scope: Scope {
            tenant: "tenant-outcomes".to_owned(),
            subject: None,
            workspace: Some("quiet".to_owned()),
            session: None,
            environment: None,
        },
        since: chrono::Utc::now() - chrono::Duration::hours(1),
        until: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    let listed = block_on(store.list_outcomes(&window)).expect("list");
    assert_eq!(listed.len(), 1, "quiet workspace history must survive");
    assert_eq!(listed[0].outcome.query_id, "q-mine");
}

#[test]
fn empty_window_is_empty_not_error() {
    let store = SqlOutcomeStore::open_in_memory().expect("store");
    let window = OutcomeWindow {
        scope: scope(),
        since: chrono::Utc::now() - chrono::Duration::hours(1),
        until: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    assert!(
        block_on(store.list_outcomes(&window))
            .expect("list")
            .is_empty()
    );
}
