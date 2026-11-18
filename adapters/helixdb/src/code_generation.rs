//! Immutable scope-bound code lineage and retained-source descriptors.
use crate::{
    HelixStore,
    code_binding::{invalid, tuple},
    records::*,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::*;
use serde_json::json;
/// One immutable association rule for reads and either order of durable preparation.
pub(crate) fn source_matches_generation(
    generation: &CodeGeneration,
    generation_id: &str,
    commit_oid: &str,
) -> bool {
    generation.id == generation_id && generation.commit_oid == commit_oid
}
impl HelixStore {
    pub(crate) fn code_immutable_generation(&self, g: &CodeGeneration) -> CoreResult<()> {
        g.validate().map_err(|_| invalid())?;
        // A future descriptor may legitimately precede its parent. Its later parent
        // must close the same invariant. select_values includes this exact step's
        // staged overlay; the shared native code-writer revision fences concurrent
        // public parent/descriptor creation together with all effects and receipt.
        for envelope in self
            .select_values(CODE_SOURCE, Some(("generationId", &g.id)))
            .map_err(core)?
        {
            let id = crate::records::id(&envelope).map_err(core)?;
            let source = self
                .code_get::<IndexedSource>(CODE_SOURCE, id)?
                .ok_or_else(invalid)?;
            if !source_matches_generation(g, &source.generation_id, &source.commit_oid) {
                return Err(invalid());
            }
        }
        if let Some(old) = self.code_get::<CodeGeneration>(CODE_GENERATION, &g.id)? {
            if old != *g {
                return Err(invalid());
            }
            return Ok(());
        }
        self.code_put(CODE_GENERATION, &g.id, g, json!({"generationId":g.id}))?;
        Ok(())
    }
    pub(crate) fn code_stage_lineage(&self, id: &str, l: &CodeLineage) -> CoreResult<()> {
        l.validate().map_err(|_| invalid())?;
        if id.is_empty() {
            return Err(invalid());
        }
        if let Some(old) = self.code_get::<TargetLineage>(CODE_LINEAGE, id)? {
            if old.repo_id != l.repo_id.0
                || old.generation_id != l.generation_id
                || old.source_path != l.source_path
                || old.span != l.span
                || old.contributors != l.contributors
            {
                return Err(invalid());
            }
            return Ok(());
        }
        let record = TargetLineage {
            record_id: id.into(),
            repo_id: l.repo_id.0.clone(),
            generation_id: l.generation_id.clone(),
            retired: false,
            source_path: l.source_path.clone(),
            span: l.span,
            contributors: l.contributors.clone(),
        };
        self.code_put(
            CODE_LINEAGE,
            id,
            &record,
            json!({"repoId":l.repo_id,"generationId":l.generation_id,"retired":false}),
        )?;
        Ok(())
    }
    pub(crate) fn code_stage_source(&self, src: &IndexedSource) -> CoreResult<()> {
        src.validate().map_err(|_| invalid())?;
        RetainedPath::new(&src.path)?;
        for parent in [
            self.code_get::<CodeGeneration>(CODE_GENERATION, &src.generation_id)?,
            self.code_get::<CodeGeneration>(
                CODE_GENERATION_OWNER,
                &tuple(&[&src.repo_id.0, &src.generation_id]),
            )?,
        ]
        .into_iter()
        .flatten()
        {
            if !source_matches_generation(&parent, &src.generation_id, &src.commit_oid) {
                return Err(invalid());
            }
        }
        let id = tuple(&[&src.repo_id.0, &src.generation_id, &src.path]);
        if let Some(old) = self.code_get::<IndexedSource>(CODE_SOURCE, &id)? {
            if old != *src {
                return Err(invalid());
            }
            return Ok(());
        }
        self.code_put(
            CODE_SOURCE,
            &id,
            src,
            json!({"repoId":src.repo_id,"generationId":src.generation_id,"path":src.path}),
        )?;
        Ok(())
    }
}
#[async_trait]
impl CodeGenerationStaging for HelixStore {
    async fn stage_lineage(&self, scope: &Scope, id: &str, l: &CodeLineage) -> CoreResult<()> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s| s.code_stage_lineage(id, l))
    }
    async fn stage_source(&self, scope: &Scope, src: &IndexedSource) -> CoreResult<()> {
        self.check_scope(scope).map_err(core)?;
        self.code_write(|s| s.code_stage_source(src))
    }
}

#[cfg(all(test, any(feature = "embedded", feature = "service")))]
mod public_source_parent_controls {
    use super::*;
    use crate::{
        StoreInner,
        test_data_fixture::*,
        transport::{Operation, Transport},
    };
    use futures::executor::block_on;
    use serde_json::Value;
    use std::{
        collections::BTreeMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };
    fn descriptor(repo: &RepoId, g: &CodeGeneration, path: &str) -> IndexedSource {
        IndexedSource {
            repo_id: repo.clone(),
            generation_id: g.id.clone(),
            commit_oid: g.commit_oid.clone(),
            path: path.into(),
            content_digest: "3".repeat(64),
        }
    }
    async fn write(store: &HelixStore, source: &IndexedSource, staged: bool) -> CoreResult<()> {
        if staged {
            store.stage_source(&scope(), source).await
        } else {
            store
                .put_source_descriptor(&scope(), source.clone())
                .await
                .map(|_| ())
        }
    }
    async fn read(store: &HelixStore, source: &IndexedSource) -> CoreResult<Option<IndexedSource>> {
        store
            .get_source_descriptor(
                &scope(),
                &source.repo_id,
                &source.generation_id,
                &source.path,
            )
            .await
    }
    // Declared test boundary: capture means NEVER FORWARDED, not an unknown outcome.
    // Injection sends that public operation once on the real engine before the other write.
    struct Boundary {
        actual: Arc<dyn Transport>,
        capture: bool,
        pending: Mutex<Option<Operation>>,
        injection: Mutex<Option<Operation>>,
        sent: AtomicUsize,
    }
    impl Transport for Boundary {
        fn execute(&self, operation: &Operation) -> crate::error::Result<Value> {
            if operation.mutates {
                if self.capture {
                    *self.pending.lock().unwrap() = Some(operation.clone());
                    return Err(crate::error::Error::Backpressure);
                }
                if let Some(injection) = self.injection.lock().unwrap().take() {
                    let response = self.actual.execute(&injection)?;
                    assert_eq!(
                        response["receipt"].as_array().unwrap().len(),
                        1,
                        "injected fresh public operation actually committed once"
                    );
                    self.sent.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.actual.execute(operation)
        }
        fn close(&self) -> crate::error::Result<()> {
            self.actual.close()
        }
        fn reconcile_read(&self, o: &Operation) -> crate::error::Result<Value> {
            self.actual.reconcile_read(o)
        }
        fn confirm_operation(&self, id: &str) -> crate::error::Result<()> {
            self.actual.confirm_operation(id)
        }
        fn register_cleanup(&self, l: crate::ownership::ScopeLease) -> crate::error::Result<()> {
            self.actual.register_cleanup(l)
        }
    }
    fn wrap(actual: &HelixStore, transport: Arc<dyn Transport>) -> HelixStore {
        HelixStore {
            inner: Arc::new(StoreInner {
                transport,
                mode: actual.inner.mode,
                partition: actual.inner.partition.clone(),
                lease: actual.inner.lease.clone(),
                reservation: AtomicBool::new(false),
                cleanup_only: AtomicBool::new(false),
                uncertain_receipts: Mutex::new(BTreeMap::new()),
            }),
            session: None,
            reentrant: false,
            step_id: None,
        }
    }
    async fn races(store: &HelixStore, repo: &RepoId, staged: bool, label: &str) {
        for parent_wins in [true, false] {
            let id = format!(
                "{label}-race-{}",
                if parent_wins { "parent" } else { "child" }
            );
            let good = code_generation(&id);
            let mut incompatible = good.clone();
            incompatible.commit_oid = "b".repeat(40);
            let source = descriptor(repo, &good, "race.rs");
            let capture = Arc::new(Boundary {
                actual: store.inner.transport.clone(),
                capture: true,
                pending: Mutex::new(None),
                injection: Mutex::new(None),
                sent: AtomicUsize::new(0),
            });
            let builder = wrap(store, capture.clone());
            let captured_result = if parent_wins {
                builder
                    .put_generation(incompatible.clone())
                    .await
                    .map(|_| ())
            } else {
                write(&builder, &source, staged).await
            };
            assert!(captured_result.is_err());
            assert_eq!(capture.sent.load(Ordering::SeqCst), 0);
            let injection = capture
                .pending
                .lock()
                .unwrap()
                .take()
                .expect("captured one never-sent public operation");
            let injected_id = injection.id.clone();
            drop(builder);
            drop(capture);
            let before = store.eligibility(&scope()).await.unwrap();
            let boundary = Arc::new(Boundary {
                actual: store.inner.transport.clone(),
                capture: false,
                pending: Mutex::new(None),
                injection: Mutex::new(Some(injection)),
                sent: AtomicUsize::new(0),
            });
            let contender = wrap(store, boundary.clone());
            let error = if parent_wins {
                write(&contender, &source, staged).await.unwrap_err()
            } else {
                contender
                    .put_generation(incompatible.clone())
                    .await
                    .unwrap_err()
            };
            assert!(
                matches!(error,CoreError::Conflict{ref reason} if reason=="GenerationChanged"),
                "native stale code-writer revision must be a certain no-effect rejection: {error}"
            );
            assert_eq!(boundary.sent.load(Ordering::SeqCst), 1);
            assert_eq!(
                store.eligibility(&scope()).await.unwrap(),
                before,
                "no public epoch effect from either preparation"
            );
            if parent_wins {
                assert_eq!(
                    store
                        .code_get::<CodeGeneration>(CODE_GENERATION, &id)
                        .unwrap(),
                    Some(incompatible.clone())
                );
                assert_eq!(
                    read(store, &source).await.unwrap(),
                    None,
                    "losing child created no unreadable row"
                );
                let recovery = descriptor(repo, &incompatible, "race.rs");
                write(store, &recovery, staged).await.unwrap();
                assert_eq!(read(store, &recovery).await.unwrap(), Some(recovery));
            } else {
                assert!(
                    store
                        .code_get::<CodeGeneration>(CODE_GENERATION, &id)
                        .unwrap()
                        .is_none(),
                    "losing parent created no contradiction"
                );
                assert_eq!(
                    read(store, &source).await.unwrap(),
                    Some(source.clone()),
                    "winning absent-parent descriptor remains readable"
                );
                store.put_generation(good.clone()).await.unwrap();
                assert_eq!(read(store, &source).await.unwrap(), Some(source));
            }
            println!(
                "TASK006 source-parent native race staged={staged} parent_wins={parent_wins}: fresh operation {injected_id} sent once; stale contender rejected/no own effects; matching explicit subsequent use succeeds; no unknown replay"
            );
            drop(contender);
            drop(boundary);
        }
    }
    fn exercise(mode: Mode) {
        block_on(async {
            let f = Fixture::new(mode);
            let store = &f.store;
            let repo = RepoId("repo".into());
            store
                .register(RepositoryRegistration {
                    repo_id: repo.clone(),
                    locator_id: "fixture".into(),
                    acquisition: AcquisitionMode::LocalOnly,
                })
                .await
                .unwrap();
            store
                .bind(CodeBinding {
                    scope: scope(),
                    repo_id: repo.clone(),
                    branch_ref: "refs/heads/main".into(),
                    version: 1,
                    active_generation: None,
                    epoch: 0,
                })
                .await
                .unwrap();
            for staged in [false, true] {
                let label = if staged { "stage" } else { "direct" };
                let future = code_generation(&format!("{label}-future"));
                let source = descriptor(&repo, &future, "future.rs");
                write(store, &source, staged).await.unwrap();
                assert_eq!(
                    read(store, &source).await.unwrap(),
                    Some(source.clone()),
                    "absent future parent is legitimate preparation"
                );
                let mut bad_parent = future.clone();
                bad_parent.commit_oid = "b".repeat(40);
                let before = store.eligibility(&scope()).await.unwrap();
                assert!(store.put_generation(bad_parent.clone()).await.is_err());
                assert!(
                    store
                        .begin_attempt(
                            &scope(),
                            &repo,
                            bad_parent,
                            code_attempt(&format!("{label}-bad-attempt"), 0, None)
                        )
                        .await
                        .is_err()
                );
                assert!(
                    store
                        .code_get::<CodeGeneration>(CODE_GENERATION, &future.id)
                        .unwrap()
                        .is_none()
                );
                assert!(
                    store
                        .get_attempt(&scope(), &repo, &format!("{label}-bad-attempt"))
                        .await
                        .unwrap()
                        .is_none()
                );
                assert_eq!(read(store, &source).await.unwrap(), Some(source.clone()));
                assert_eq!(store.eligibility(&scope()).await.unwrap(), before);
                store.put_generation(future.clone()).await.unwrap();
                assert_eq!(read(store, &source).await.unwrap(), Some(source));
                let present = code_generation(&format!("{label}-present"));
                store.put_generation(present.clone()).await.unwrap();
                let good = descriptor(&repo, &present, "present.rs");
                let mut bad = good.clone();
                bad.commit_oid = "b".repeat(40);
                assert!(write(store, &bad, staged).await.is_err());
                assert_eq!(
                    read(store, &bad).await.unwrap(),
                    None,
                    "known mismatch must not be durably acknowledged or poison the key"
                );
                assert_eq!(
                    store
                        .code_get::<CodeGeneration>(CODE_GENERATION, &present.id)
                        .unwrap(),
                    Some(present)
                );
                write(store, &good, staged).await.unwrap();
                assert_eq!(read(store, &good).await.unwrap(), Some(good));
                for child_first in [true, false] {
                    let generation = code_generation(&format!(
                        "{label}-step-{}",
                        if child_first { "child" } else { "parent" }
                    ));
                    let source = descriptor(&repo, &generation, "step.rs");
                    let session = store.scope_session().unwrap();
                    let bound = session.store();
                    let step = bound.begin_step().unwrap();
                    if child_first {
                        write(&bound, &source, staged).await.unwrap();
                        let mut incompatible = generation.clone();
                        incompatible.commit_oid = "b".repeat(40);
                        assert!(bound.put_generation(incompatible).await.is_err());
                        assert!(
                            bound
                                .code_get::<CodeGeneration>(CODE_GENERATION, &generation.id)
                                .unwrap()
                                .is_none()
                        );
                        bound.put_generation(generation.clone()).await.unwrap();
                    } else {
                        bound.put_generation(generation.clone()).await.unwrap();
                        let mut incompatible = source.clone();
                        incompatible.commit_oid = "b".repeat(40);
                        assert!(write(&bound, &incompatible, staged).await.is_err());
                        assert_eq!(read(&bound, &incompatible).await.unwrap(), None);
                        write(&bound, &source, staged).await.unwrap();
                    }
                    assert_eq!(
                        read(&bound, &source).await.unwrap(),
                        Some(source.clone()),
                        "staged read sees coherent final overlay"
                    );
                    step.commit().unwrap();
                    drop(bound);
                    drop(session);
                    assert_eq!(read(store, &source).await.unwrap(), Some(source));
                    assert_eq!(
                        store
                            .code_get::<CodeGeneration>(CODE_GENERATION, &generation.id)
                            .unwrap(),
                        Some(generation)
                    );
                }
                races(store, &repo, staged, label).await;
            }
            println!(
                "TASK006 {mode:?}: direct/stage_source absent/present/mismatch/no-effect/recovery, reverse parent creation, both staged orders and both engine-level public races verified"
            );
        });
    }
    #[cfg(feature = "embedded")]
    #[test]
    fn embedded_public_source_parent_write_and_race_coherence() {
        exercise(Mode::Embedded)
    }
    #[cfg(feature = "service")]
    #[test]
    fn service_public_source_parent_write_and_race_coherence() {
        exercise(Mode::Service)
    }
}
