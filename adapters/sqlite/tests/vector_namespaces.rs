//! Representation isolation honors a shared SQLite file and legacy vector data.
use engram_domain::{EmbeddingSpace, Id};
use engram_retrieval::VectorIndex;
use engram_store_sqlite::SqliteVectorIndex;
use futures::executor::block_on;

#[test]
fn namespaces_keep_identical_target_ids_and_clear_gc_metadata_independent() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.db");
    let path = path.to_str().unwrap();
    let legacy = SqliteVectorIndex::open(path, 3).unwrap();
    let space_a = EmbeddingSpace::new("test", "model", 3, "representation-a", None::<String>);
    let space_b = EmbeddingSpace::new("test", "model", 3, "representation-b", None::<String>);
    let a = SqliteVectorIndex::open_with_embedding_space_namespace(
        path,
        space_a.clone(),
        &"a".repeat(64),
    )
    .unwrap();
    let b = SqliteVectorIndex::open_with_embedding_space_namespace(
        path,
        space_b.clone(),
        &"b".repeat(64),
    )
    .unwrap();
    let id = Id::from("same-target");
    for (index, space, vector) in [
        (&legacy, legacy.embedding_space(), vec![1., 0., 0.]),
        (&a, &space_a, vec![0., 1., 0.]),
        (&b, &space_b, vec![0., 0., 1.]),
    ] {
        block_on(VectorIndex::insert(index, &id, space, vector)).unwrap();
    }
    assert_eq!(a.search(&[0., 1., 0.], 1).unwrap()[0].distance, 0.);
    assert_eq!(b.search(&[0., 1., 0.], 1).unwrap()[0].distance, 2f32.sqrt());
    assert_eq!(
        block_on(a.vector_for_target(&id)).unwrap(),
        Some(vec![0., 1., 0.])
    );
    assert_eq!(
        block_on(b.vector_for_target(&id)).unwrap(),
        Some(vec![0., 0., 1.])
    );
    assert_eq!(block_on(a.embedded_ids()).unwrap().len(), 1);
    assert_eq!(block_on(a.gc_orphan_targets(&[])).unwrap(), 1);
    assert!(block_on(a.embedded_ids()).unwrap().is_empty());
    assert_eq!(block_on(b.embedded_ids()).unwrap().len(), 1);
    SqliteVectorIndex::clear(&b).unwrap();
    assert_eq!(
        block_on(legacy.vector_for_target(&id)).unwrap(),
        Some(vec![1., 0., 0.])
    );
    let reopened =
        SqliteVectorIndex::open_with_embedding_space_namespace(path, space_b, &"a".repeat(64))
            .unwrap();
    assert!(reopened.requires_reindex());
    assert_eq!(reopened.embedding_space(), &space_a);
    assert!(reopened.search(&[1., 0., 0.], 1).is_err());
    assert!(block_on(reopened.vector_for_target(&id)).is_err());
    assert!(
        block_on(VectorIndex::insert(
            &reopened,
            &id,
            &space_a,
            vec![1., 0., 0.]
        ))
        .is_err()
    );
}

#[test]
fn namespace_rejects_untrusted_identifiers_before_opening_the_database() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("not-created.db");
    let space = EmbeddingSpace::new("test", "model", 3, "profile", None::<String>);
    for value in [
        "",
        "../escape",
        "vectors; DROP TABLE secrets",
        &"A".repeat(64),
    ] {
        assert!(
            SqliteVectorIndex::open_with_embedding_space_namespace(
                path.to_str().unwrap(),
                space.clone(),
                value
            )
            .is_err()
        );
        assert!(!path.exists());
    }
}

#[test]
fn occupied_namespace_without_identity_is_not_adopted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.db");
    let path = path.to_str().unwrap();
    let space = EmbeddingSpace::new("test", "model", 3, "profile", None::<String>);
    let digest = "c".repeat(64);
    let index =
        SqliteVectorIndex::open_with_embedding_space_namespace(path, space.clone(), &digest)
            .unwrap();
    block_on(VectorIndex::insert(
        &index,
        &Id::from("target"),
        &space,
        vec![1., 0., 0.],
    ))
    .unwrap();
    drop(index);
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(&format!("DELETE FROM vector_meta_{digest}"), [])
        .unwrap();
    drop(conn);
    assert!(SqliteVectorIndex::open_with_embedding_space_namespace(path, space, &digest).is_err());
}
