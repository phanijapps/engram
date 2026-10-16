//! Explicit standalone KG hard deletes retain their existing lifetime semantics.
use rusqlite::{Connection, params};

#[test]
fn unowned_kg_updates_and_hard_deletes_do_not_create_source_history() {
    let db = Connection::open_in_memory().unwrap();
    super::schema::initialize_schema(&db).unwrap();
    for table in [
        "knowledge_entities_records",
        "knowledge_relationships_records",
        "knowledge_graphs_records",
    ] {
        db.execute(
            &format!("INSERT INTO {table}(id,tenant,record_json) VALUES('standalone','t',?1)"),
            [r#"{"id":"standalone","body":"private unrelated fact"}"#],
        )
        .unwrap();
        db.execute(
            &format!("UPDATE {table} SET record_json=?1 WHERE id='standalone'"),
            [r#"{"id":"standalone","body":"edited unrelated fact"}"#],
        )
        .unwrap();
        db.execute(
            &format!("DELETE FROM {table} WHERE id=?1"),
            params!["standalone"],
        )
        .unwrap();
    }
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM canonical_document_history",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "source refresh must not retain arbitrary KG hard deletes"
    );
}

#[test]
fn code_only_mutations_archive_exact_state_once_and_skip_noops() {
    let db = Connection::open_in_memory().unwrap();
    super::schema::initialize_schema(&db).unwrap();
    // Reopening an existing cell must install the same correction idempotently.
    super::schema::initialize_schema(&db).unwrap();
    for (table, kind) in [
        ("knowledge_graphs_records", "graph"),
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ] {
        let old = format!(r#"{{"id":"{kind}","status":"pending"}}"#);
        let new = format!(r#"{{"id":"{kind}","status":"resolved"}}"#);
        let columns = if kind == "unresolved_reference" {
            "id,tenant,record_json,status"
        } else {
            "id,tenant,record_json"
        };
        let values = if kind == "unresolved_reference" {
            "?1,'t',?2,'pending'"
        } else {
            "?1,'t',?2"
        };
        db.execute(
            &format!("INSERT INTO {table}({columns}) VALUES({values})"),
            params![kind, old],
        )
        .unwrap();
        db.execute(
            "INSERT INTO code_source_contributions VALUES('file','attempt',?1,?1)",
            [kind],
        )
        .unwrap();
        db.execute(
            &format!("UPDATE {table} SET record_json=record_json WHERE id=?1"),
            [kind],
        )
        .unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM canonical_document_history", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            count,
            2 * ["graph", "entity", "relationship", "unresolved_reference"]
                .iter()
                .position(|v| *v == kind)
                .unwrap() as i64
        );
        db.execute(
            &format!("UPDATE {table} SET record_json=?2 WHERE id=?1"),
            params![kind, new],
        )
        .unwrap();
        // A dual-owned record must use the existing head archive only.
        db.execute(
            "INSERT INTO document_head_records VALUES('head','attempt',?1,?1)",
            [kind],
        )
        .unwrap();
        db.execute(&format!("DELETE FROM {table} WHERE id=?1"), [kind])
            .unwrap();
        let states: Vec<String> = db.prepare("SELECT record_json FROM canonical_document_history WHERE kind=?1 AND record_id=?1 ORDER BY id").unwrap().query_map([kind],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(states, vec![old, new]);
    }
}
