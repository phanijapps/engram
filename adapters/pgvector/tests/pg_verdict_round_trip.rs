//! FIND-code-101 regression: the pgvector memory write surface persists
//! verdicts and rejects the reserved `derived` word through the shared
//! engine-neutral validation (`engram_memory::validate_write_request`).
//!
//! Requires a running Postgres with pgvector
//! (docker compose -f docs/how-to-pg/docker-compose.yaml up -d).
//! Run: cargo test -p engram-store-pgvector -- --ignored pg_verdict

#[allow(dead_code)]
const CONN: &str = "postgres://engram:engram@localhost:5432/engram";

use chrono::Utc;
use engram_domain::{
    Actor, ActorId, ActorKind, MemoryContent, MemoryKind, Policy, Provenance, Requester, Retention,
    Scope, Verdict, Visibility, WriteMemoryRequest,
};
use engram_memory::{MemoryRepository as _, MemoryService as _};
use engram_runtime::CoreError;
use engram_store_pgvector::{schema, PgConnection, PgMemoryService};

fn verdict_request(scope: &Scope, verdict: Option<Verdict>) -> WriteMemoryRequest {
    let now = Utc::now();
    let actor = Actor {
        id: ActorId::from("pg-verdict-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    };
    WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "pg verdict round-trip fact".to_owned(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope.clone(),
        requester: Requester {
            actor: actor.clone(),
            roles: Vec::new(),
            permissions: Vec::new(),
            on_behalf_of: None,
        },
        provenance: Provenance {
            source: "pg-verdict-test".to_owned(),
            actor,
            observed_at: now,
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
        verdict,
        idempotency_key: None,
    }
}

#[test]
#[ignore]
fn pg_verdict_write_persists_and_derived_is_rejected() {
    // Raw SQL goes through a connection's shared runtime (pg_round_trip
    // pattern); async service methods are driven with the executor.
    let setup = PgConnection::connect(CONN).expect("connect to pgvector");
    setup.block_on(async {
        setup
            .client
            .batch_execute(&schema::schema_sql_without_vectors())
            .await
            .expect("apply schema")
    });
    let scope = Scope {
        tenant: "pg-verdict-test".to_owned(),
        subject: None,
        workspace: Some("verdict".to_owned()),
        session: None,
        environment: None,
    };
    let service = PgMemoryService::new(PgConnection::connect(CONN).expect("connect to pgvector"));

    // 1. Direct write persists the verdict (FIND-code-101: the record used
    //    to hard-code None and silently discard every verdict on pgvector).
    let response = futures::executor::block_on(
        service.write_memory(verdict_request(&scope, Some(Verdict::GroundedInSource))),
    )
    .expect("write with verdict");
    assert_eq!(
        response.record.verdict,
        Some(Verdict::GroundedInSource),
        "response record carries the verdict"
    );

    // Read-back: the persisted record keeps it.
    let stored = futures::executor::block_on(service.get_memory(&response.record.id, &scope))
        .expect("get_memory")
        .expect("record present");
    assert_eq!(
        stored.verdict,
        Some(Verdict::GroundedInSource),
        "pgvector persists the verdict (was silently dropped pre-fix)"
    );

    // 2. The reserved word is rejected with the reserved-for-derivation
    //    error via the SAME shared seam the sqlite path rides.
    let err = futures::executor::block_on(
        service.write_memory(verdict_request(&scope, Some(Verdict::Derived))),
    )
    .expect_err("derived must be rejected on pgvector too");
    match err {
        CoreError::InvalidRequest { reason } => assert!(
            reason.contains("reserved"),
            "reserved-for-derivation reason: {reason}"
        ),
        other => panic!("expected InvalidRequest, got {other:?}"),
    }

    // 3. The other writable word is accepted and persisted.
    let ok = futures::executor::block_on(
        service.write_memory(verdict_request(&scope, Some(Verdict::AssertedByAgent))),
    )
    .expect("asserted_by_agent accepted");
    assert_eq!(ok.record.verdict, Some(Verdict::AssertedByAgent));

    // Clean up.
    let cleanup = PgConnection::connect(CONN).expect("connect to pgvector");
    cleanup.block_on(async {
        cleanup
            .client
            .execute(
                "DELETE FROM memories WHERE tenant = $1",
                &[&"pg-verdict-test"],
            )
            .await
            .expect("clean memories")
    });
    let cleanup_events = PgConnection::connect(CONN).expect("connect to pgvector");
    cleanup_events.block_on(async {
        cleanup_events
            .client
            .execute(
                "DELETE FROM memory_events WHERE tenant = $1",
                &[&"pg-verdict-test"],
            )
            .await
            .expect("clean events")
    });
}
