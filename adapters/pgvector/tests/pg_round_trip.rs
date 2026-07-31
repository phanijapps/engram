//! Integration test: write + read memories against the Docker pgvector instance.
//!
//! Requires a running Postgres with pgvector (docker compose -f docs/how-to-pg/docker-compose.yaml up -d).
//! Run: cargo test -p engram-store-pgvector -- --ignored pg_round_trip

#[allow(dead_code)]
const CONN: &str = "postgres://engram:engram@localhost:5432/engram";

#[test]
#[ignore]
fn pg_memory_write_read_round_trip() {
    use chrono::Utc;
    use engram_store_pgvector::{schema, PgConnection, PgMemoryRow};

    let conn = PgConnection::connect(CONN).expect("connect to pgvector");

    // Apply the schema (idempotent).
    conn.block_on(async {
        conn.client
            .batch_execute(&schema::schema_sql(384))
            .await
            .expect("apply schema")
    });

    // Clean slate for this test.
    conn.block_on(async {
        conn.client
            .execute(
                "DELETE FROM memories WHERE tenant = $1",
                &[&"round-trip-test"],
            )
            .await
            .expect("clean")
    });

    let now = Utc::now();

    // Write two memories under tenant "round-trip-test".
    let m1 = PgMemoryRow {
        id: "rt-1".to_owned(),
        content: "First memory from the pgvector adapter.".to_owned(),
        tenant: "round-trip-test".to_owned(),
        workspace: Some("test".to_owned()),
        created_at: now,
    };
    let m2 = PgMemoryRow {
        id: "rt-2".to_owned(),
        content: "Second memory — newer.".to_owned(),
        tenant: "round-trip-test".to_owned(),
        workspace: Some("test".to_owned()),
        created_at: now + chrono::Duration::seconds(10),
    };
    engram_store_pgvector::write_memory(&conn, &m1).expect("write m1");
    engram_store_pgvector::write_memory(&conn, &m2).expect("write m2");

    // Read recent for the test tenant — should get both, newest-first.
    let rows =
        engram_store_pgvector::read_recent(&conn, "round-trip-test", 10).expect("read recent");
    assert_eq!(rows.len(), 2, "two memories for this tenant");
    assert_eq!(rows[0].id, "rt-2", "newest first");
    assert_eq!(rows[1].id, "rt-1");

    // Scope isolation: a different tenant sees nothing.
    let other = engram_store_pgvector::read_recent(&conn, "other-tenant", 10).expect("read other");
    assert!(
        other.is_empty(),
        "scope isolation: other tenant sees nothing"
    );

    // Clean up.
    conn.block_on(async {
        conn.client
            .execute(
                "DELETE FROM memories WHERE tenant = $1",
                &[&"round-trip-test"],
            )
            .await
            .expect("clean")
    });

    println!("pgvector memory round-trip: write → read → scope-isolation ✓");
}
