//! Real Postgres tests isolated in disposable schemas; no shared rows are deleted.
//! Run with local Docker Postgres: cargo test -p engram-backend-pgvector --test embedding_space -- --ignored
use engram_domain::{EmbeddingSpace, Id};
use engram_retrieval::VectorIndex;
use engram_store_pgvector::{PgConnection, PgVectorIndex, schema};
use futures::executor::block_on;
use std::sync::atomic::{AtomicUsize, Ordering};

const CONNECTION: &str = "postgres://engram:engram@localhost:5432/engram";
struct Fixture {
    admin: PgConnection,
    schema: String,
}
impl Fixture {
    fn new(dimensions: u32) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let schema = format!(
            "embedding_config_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let admin = PgConnection::connect(CONNECTION).unwrap();
        admin
            .block_on(
                admin
                    .client
                    .batch_execute(&format!("CREATE SCHEMA {schema}")),
            )
            .unwrap();
        let fixture = Self { admin, schema };
        let conn = fixture.connection();
        conn.block_on(conn.client.batch_execute(&schema::schema_sql(dimensions)))
            .unwrap();
        fixture
    }
    fn connection(&self) -> PgConnection {
        let conn = PgConnection::connect(CONNECTION).unwrap();
        conn.block_on(
            conn.client
                .batch_execute(&format!("SET search_path TO {}, public", self.schema)),
        )
        .unwrap();
        conn
    }
    fn count(&self) -> i64 {
        let conn = self.connection();
        conn.block_on(conn.client.query_one("SELECT count(*) FROM vectors", &[]))
            .unwrap()
            .get(0)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.admin
            .block_on(
                self.admin
                    .client
                    .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.schema)),
            )
            .unwrap();
    }
}
fn space(model: &str, dimensions: u32) -> EmbeddingSpace {
    EmbeddingSpace::new("fixture", model, dimensions, "query", None::<String>)
}
fn expect_error(result: engram_runtime::CoreResult<PgVectorIndex>) -> String {
    match result {
        Ok(_) => panic!("incompatible index must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn persisted_space_fences_same_dimension_model_instruction_and_dimension_changes() {
    let fixture = Fixture::new(3);
    let original = space("model-a", 3);
    let index = PgVectorIndex::new_checked(fixture.connection(), original.clone()).unwrap();
    block_on(index.insert(&Id::from("kept-vector"), &original, vec![1., 0., 0.])).unwrap();
    assert!(
        expect_error(PgVectorIndex::new_checked(
            fixture.connection(),
            space("model-b", 3)
        ))
        .contains("reindex")
    );
    assert!(
        expect_error(PgVectorIndex::new_checked(
            fixture.connection(),
            space("model-a", 4)
        ))
        .contains("reindex")
    );
    let mut changed = original.clone();
    changed.prompt_profile = "query#changed-instructions".into();
    assert!(
        expect_error(PgVectorIndex::new_checked(fixture.connection(), changed)).contains("reindex")
    );
    assert_eq!(fixture.count(), 1);
    let reopened = PgVectorIndex::new_checked(fixture.connection(), original.clone()).unwrap();
    assert_eq!(
        block_on(reopened.search(&original, vec![1., 0., 0.], 1)).unwrap()[0].0,
        Id::from("kept-vector")
    );
    assert!(
        block_on(reopened.insert(&Id::from("bad"), &original, vec![f32::NAN, 0., 0.])).is_err()
    );
    assert!(block_on(reopened.search(&original, vec![1.], 1)).is_err());
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn legacy_vectors_are_adopted_only_for_provable_default_bge_space() {
    let fixture = Fixture::new(384);
    let default = EmbeddingSpace::new(
        "fastembed",
        "BAAI/bge-small-en-v1.5",
        384,
        "query",
        None::<String>,
    );
    let legacy = PgVectorIndex::new(fixture.connection(), default.clone());
    block_on(legacy.insert(&Id::from("legacy"), &default, vec![0.1; 384])).unwrap();
    let mut wrong = default.clone();
    wrong.prompt_profile = "passage".into();
    assert!(PgVectorIndex::new_checked(fixture.connection(), wrong).is_err());
    assert_eq!(fixture.count(), 1);
    PgVectorIndex::new_checked(fixture.connection(), default).unwrap();
    assert_eq!(fixture.count(), 1);
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn unlabelled_or_nondefault_legacy_vectors_are_not_relabelled() {
    let fixture = Fixture::new(3);
    let conn = fixture.connection();
    conn.block_on(conn.client.execute(
        "INSERT INTO vectors (id,embedding) VALUES ('unlabelled', '[1,0,0]'::vector)",
        &[],
    ))
    .unwrap();
    assert!(PgVectorIndex::new_checked(fixture.connection(), space("model", 3)).is_err());
    assert_eq!(fixture.count(), 1);
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn wider_vectors_use_exact_search_without_hnsw() {
    let fixture = Fixture::new(2048);
    let original = space("wide", 2048);
    let index = PgVectorIndex::new_checked(fixture.connection(), original.clone()).unwrap();
    block_on(index.insert(&Id::from("wide"), &original, vec![0.1; 2048])).unwrap();
    assert_eq!(
        block_on(index.search(&original, vec![0.1; 2048], 1))
            .unwrap()
            .len(),
        1
    );
    // A later differently configured open must not attempt an illegal HNSW build.
    let conn = fixture.connection();
    conn.block_on(conn.client.batch_execute(&schema::schema_sql(384)))
        .unwrap();
    assert!(PgVectorIndex::new_checked(fixture.connection(), space("small", 384)).is_err());
    assert_eq!(fixture.count(), 1);
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn disabled_vectors_do_not_contact_configured_http_provider() {
    use engram_domain::{CapabilityState, ScopeMappingStrategy};
    use engram_integration::{
        CapabilityPolicy, EmbeddingOptions, EmbeddingProviderConfig, EngramConfig, MigrationMode,
    };
    let fixture = Fixture::new(3);
    let mut config = EngramConfig::new(
        std::env::temp_dir().join("unused-pg-embedding.db"),
        std::env::temp_dir(),
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "ollama".into(),
            model: "must-not-be-loaded".into(),
            dimensions: 0,
            options: EmbeddingOptions {
                endpoint: Some("http://127.0.0.1:1".into()),
                ..Default::default()
            },
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_pgvector(format!(
        "{CONNECTION}?options=-csearch_path%3D{},public",
        fixture.schema
    ));
    config.enable_vector = false;
    let provider = engram_backend_pgvector::open(&config).unwrap();
    assert!(matches!(
        provider.capabilities().vectors,
        CapabilityState::Unsupported { .. }
    ));
    assert!(provider.require_vectors().is_err());
    assert!(provider.embedding_provider().is_none());
    assert_eq!(fixture.count(), 0);
}

#[test]
#[ignore = "requires local Docker Postgres"]
fn concurrent_first_open_selects_one_persisted_space() {
    let fixture = Fixture::new(3);
    let a = fixture.connection();
    let b = fixture.connection();
    let first = std::thread::spawn(move || PgVectorIndex::new_checked(a, space("model-a", 3)));
    let second = std::thread::spawn(move || PgVectorIndex::new_checked(b, space("model-b", 3)));
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    assert_ne!(
        first.is_ok(),
        second.is_ok(),
        "exactly one concurrent model is adopted"
    );
    assert_eq!(fixture.count(), 0);
}
