//! Helix recipe: retained physical owners plus canonical core/integration services.
pub mod composition;
use composition::{HelixComposition, KnowledgePorts};
use engram_domain::*;
use engram_integration::storage_composition::{ArchiveInventory, CompositionRead};
use engram_integration::{CapabilityReport, EngramConfig, EngramProvider, EngramProviderBuilder};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use engram_store_helixdb::HelixStore;
use std::sync::Arc;

struct Clock;
impl engram_runtime::Clock for Clock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now()
    }
}
struct Ids;
impl engram_runtime::IdGenerator for Ids {
    fn new_id(&self, _: &'static str) -> Id {
        uuid::Uuid::new_v4().to_string().into()
    }
}
fn local_error() -> CoreError {
    CoreError::Adapter {
        adapter: "helixdb-recipe".into(),
        message: "recipe local resource unavailable".into(),
    }
}

/// Compose existing core ports. The caller supplies policy; no implicit actor grant
/// or alternate storage provider is selected here.
pub fn compose(
    store: HelixStore,
    config: &EngramConfig,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    if config.rebuild_vectors {
        return Err(CoreError::CapabilityUnsupported {
            capability: "rebuild_vectors".into(),
            reason: "use the existing governed reset coordinator".into(),
        });
    }
    let storage =
        engram_store_helixdb::config::confined_path(&config.trusted_root, &config.storage_path)
            .map_err(|_| local_error())?;
    std::fs::create_dir_all(&storage).map_err(|_| local_error())?;
    let retained = engram_store_helixdb::config::confined_path(
        &config.trusted_root,
        &storage.join("retained"),
    )
    .map_err(|_| local_error())?;
    let lexical_path =
        engram_store_helixdb::config::confined_path(&config.trusted_root, &storage.join("lexical"))
            .map_err(|_| local_error())?;
    std::fs::create_dir_all(&lexical_path).map_err(|_| local_error())?;
    let prepared =
        engram_integration::prepare_embedding(&config.embedding_provider, config.enable_vector)?;
    let scope = store.composition_scope().clone();
    let native = Arc::new(store.clone());
    let knowledge = Arc::new(KnowledgePorts::new(store.clone()));
    let memory: Arc<dyn engram_memory::MemoryService> =
        Arc::new(store.memory_service(authorizer.clone()));
    let beliefs: Arc<dyn engram_belief::BeliefRepository> = native.clone();
    let bindings: Arc<dyn engram_knowledge::CodeBindingRepository> = native.clone();
    let eligibility: Arc<dyn engram_retrieval::CodeEligibilityResolver> = Arc::new(
        engram_integration::code_binding::RepositoryEligibilitySource::new(bindings.clone()),
    );
    let lexical = Arc::new(
        engram_store_lexical::LexicalIndex::open(&lexical_path).map_err(|_| local_error())?,
    );
    let feed: Arc<dyn engram_integration::LexicalFeed> = Arc::new(LexicalFeed(lexical.clone()));
    let read: Arc<dyn CompositionRead> = knowledge.clone();
    let resolver = Arc::new(LexicalResolver(
        engram_integration::recall_lanes::KnowledgeTargetResolver::new(read.clone())
            .deny_unbound_symbols(),
    ));
    let lexical_lane: Arc<dyn engram_retrieval::RetrievalIndex> = Arc::new(
        engram_store_lexical::LexicalRetrievalIndex::from_arc_with_eligibility(
            lexical,
            resolver,
            eligibility.clone(),
        ),
    );
    let mut native_retrieval = store.retrieval_index(authorizer.clone());
    if let Some(provider) = prepared.provider.clone() {
        if store.composition_embedding_space() != Some(&provider.embedding_space()) {
            return Err(CoreError::InvalidRequest {
                reason: "embedding provider differs from selected native space".into(),
            });
        }
        native_retrieval =
            native_retrieval.with_query_vector_provider(Arc::new(QueryProvider(provider)));
    }
    let retrieval: Arc<dyn engram_retrieval::RetrievalIndex> = Arc::new(native_retrieval);
    let graph_source: Arc<dyn engram_store_associative_graph::GraphRelationshipSource> =
        Arc::new(GraphSource(read.clone()));
    let lanes = vec![
        retrieval.clone(),
        lexical_lane,
        Arc::new(engram_store_associative_graph::AssociativeGraphIndex::new(
            graph_source.clone(),
        )) as Arc<dyn engram_retrieval::RetrievalIndex>,
        Arc::new(engram_store_community_summary::CommunitySummaryIndex::new(
            graph_source,
        )) as Arc<dyn engram_retrieval::RetrievalIndex>,
        engram_integration::recall_lanes::procedures_recall_lane(native.clone()),
    ];
    let fusion = config
        .recall_fusion
        .as_ref()
        .map(|c| c.to_reciprocal_config())
        .transpose()?
        .unwrap_or_default();
    let recall = engram_integration::recall_composition::SharedUnifiedRecall::with_reranker(
        memory.clone(),
        lanes,
        beliefs.clone(),
        fusion,
        None,
    )
    .with_code_eligibility(eligibility);
    let recall = recall.with_candidate_evidence(Arc::new(
        engram_integration::candidate_evidence::RepositoryCandidateEvidence::new(
            memory.clone(),
            beliefs.clone(),
            knowledge.clone(),
            authorizer.clone(),
            Some(bindings.clone()),
            Some(native.clone()),
        ),
    ));
    let cache: Arc<dyn engram_retrieval::GraphCache> =
        Arc::new(engram_retrieval::InMemoryGraphCache::default());
    let coordinator = engram_integration::code_binding::CodeBindingCoordinator::new(
        bindings.clone(),
        Arc::new(engram_integration::code_binding::RecipeDefaultAuthorizer),
        vec![],
    )
    .with_reset_lanes(Arc::new(
        engram_integration::code_binding::RecipeResetLaneLifecycle::new(
            Some(feed.clone()),
            None,
            cache.clone(),
        ),
    ))
    .with_preservation_source(Arc::new(
        engram_integration::code_binding::KnowledgePreservationRecordSource::new(
            knowledge.clone(),
            knowledge.clone(),
            bindings,
        ),
    ));
    let mut report = CapabilityReport::new(CapabilityState::Supported);
    let vector = if config.enable_vector && store.composition_embedding_space().is_some() {
        Some(store.vector_index()?)
    } else {
        None
    };
    if vector.is_none() {
        report.vectors = CapabilityState::Unsupported {
            reason: CapabilityReason::FeatureDisabled,
        };
    }
    let observable = engram_integration::observability_composition::SharedObservability::new(
        Some(read.clone()),
        Some(beliefs.clone()),
        scope.clone(),
        report.clone(),
        prepared.config.clone(),
        "1",
        "0.1.0",
    );
    let mut builder = EngramProviderBuilder::new(report)
        .memory(memory.clone())
        .knowledge(native.clone())
        .graph(native.clone())
        .graph_maintenance(native.clone())
        .ontology(native.clone())
        .taxonomy(native.clone())
        .beliefs(beliefs.clone())
        .hierarchy(native.clone())
        .procedures(native.clone())
        .identity(native.clone())
        .outcomes(native.clone())
        .change_log(native.clone())
        .checkpoints(native.clone())
        .proposals(Arc::new(engram_proposals::ProposalService::new(
            memory.clone(),
            native.clone(),
            Arc::new(engram_proposals::DenyAllProposalAuthorizer),
        )))
        .retrieval(retrieval)
        .recall(Arc::new(recall))
        .knowledge_query(knowledge.clone())
        .community_query(Arc::new(
            engram_integration::community_composition::SharedCommunityQuery::new(read),
        ))
        .provenance(Arc::new(
            engram_integration::provenance_composition::SharedProvenanceQuery::new(
                knowledge.clone(),
            ),
        ))
        .export_import(Arc::new(
            engram_integration::archive_composition::ArchiveExport::new(
                knowledge.clone() as Arc<dyn ArchiveInventory>
            ),
        ))
        .migration(Arc::new(
            engram_integration::migration_validation::CanonicalMigrationService::new(
                prepared.config.dimensions,
            ),
        ))
        .observability(Arc::new(observable))
        .lexical_feed(feed)
        .graph_cache(cache)
        .code_bindings(Arc::new(coordinator))
        .code_source(Arc::new(
            engram_ingest::git_source::RetainedSnapshotReader::new(retained.clone()),
        ))
        .code_ingestion(Arc::new(
            engram_integration::code_binding::CodeIngestionLanes {
                knowledge: knowledge.clone(),
                graph: native.clone(),
                retained_root: retained,
            },
        ))
        .scope_inventory(Arc::new(Inventory(store.clone())))
        .schema_version("1")
        .adapter_version("0.1.0");
    builder = builder
        .batch(Arc::new(
            engram_integration::batch_runtime::SharedBatchIngest::new(Arc::new(
                HelixComposition::new(store.clone(), authorizer),
            )),
        ))
        .consolidation(Arc::new(consolidation(memory, beliefs)));
    if let Some(vector) = vector {
        builder = builder.vectors(Arc::new(vector));
    }
    if let Some(embedding) = prepared.provider {
        builder = builder.embedding_provider(embedding);
    }
    Ok(builder.build())
}

struct Detector;
#[async_trait::async_trait]
impl engram_belief::ContradictionDetector for Detector {
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>> {
        engram_belief::contradiction::detect_belief_contradictions(beliefs, chrono::Utc::now())
    }
}
fn consolidation(
    memory: Arc<dyn engram_memory::MemoryService>,
    beliefs: Arc<dyn engram_belief::BeliefRepository>,
) -> engram_integration::consolidation_composition::ExecutorConsolidationService {
    use engram_integration::consolidation_composition::*;
    let source = Arc::new(ActiveMemorySourceAdapter(memory.clone()));
    let sink = Arc::new(BeliefSinkAdapter(beliefs.clone()));
    let synthesizer = Arc::new(engram_reflection::PatternSynthesizer::new(
        source,
        chrono::Utc::now(),
    ));
    let executors: Vec<Arc<dyn engram_consolidation::ConsolidationMutationExecutor>> = vec![
        Arc::new(engram_reflection::ReflectionExecutor::new(
            synthesizer,
            sink,
        )),
        Arc::new(engram_decay::DecayExecutor::new(Arc::new(
            DecayMemorySourceAdapter(memory.clone()),
        ))),
        Arc::new(engram_reflection::ContradictionExecutor::new(
            Arc::new(Detector),
            beliefs,
        )),
        Arc::new(engram_reflection::MemoryLifecycleExecutor::with_default_threshold(memory)),
    ];
    ExecutorConsolidationService::new(
        Arc::new(Clock),
        Arc::new(Ids),
        Arc::new(engram_consolidation::CompositeConsolidationExecutor::new(
            executors,
        )),
    )
}
struct GraphSource(Arc<dyn CompositionRead>);
#[async_trait::async_trait]
impl engram_store_associative_graph::GraphRelationshipSource for GraphSource {
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.0.list_entities(scope).await
    }
    async fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.0.list_relationships(scope).await
    }
}
struct LexicalResolver(engram_integration::recall_lanes::KnowledgeTargetResolver);
#[async_trait::async_trait]
impl engram_store_lexical::LexicalTargetResolver for LexicalResolver {
    async fn resolve(
        &self,
        id: &str,
        request: &RetrievalRequest,
    ) -> CoreResult<Option<engram_store_lexical::LexicalResolvedTarget>> {
        Ok(self.0.resolve(id, request).await?.map(|t| {
            engram_store_lexical::LexicalResolvedTarget {
                target_type: t.target_type,
                target_id: t.target_id,
                content: t.content,
                provenance: t.provenance,
                policy: t.policy,
                explanation: t.explanation,
                metadata: t.metadata,
                code_kind: t.code_kind,
            }
        }))
    }
    async fn eligible_target_ids(
        &self,
        request: &RetrievalRequest,
        view: &engram_retrieval::BoundCodeView,
    ) -> CoreResult<std::collections::BTreeSet<String>> {
        self.0.eligible_target_ids(request, view).await
    }
}
struct LexicalFeed(Arc<engram_store_lexical::LexicalIndex>);
#[async_trait::async_trait]
impl engram_integration::LexicalFeed for LexicalFeed {
    async fn upsert(&self, id: &str, text: &str) -> CoreResult<()> {
        self.0.upsert(id, text).map_err(|_| local_error())
    }
    async fn upsert_batch(&self, entries: &[(String, String)]) -> CoreResult<()> {
        self.0.upsert_batch(entries).map_err(|_| local_error())
    }
    async fn upsert_code_target(&self, id: &str, text: &str, generation: &str) -> CoreResult<()> {
        self.0
            .upsert_stamped(
                id,
                text,
                &engram_store_lexical::code_contributor_key(generation),
            )
            .map_err(|_| local_error())
    }
    async fn retire_generation(&self, generation: &str) -> CoreResult<usize> {
        self.0
            .retire_contributor(&engram_store_lexical::code_contributor_key(generation))
            .map_err(|_| local_error())
    }
    async fn retire_code_target(&self, id: &str, generation: &str) -> CoreResult<usize> {
        self.0
            .retire_target_contributor(id, &engram_store_lexical::code_contributor_key(generation))
            .map_err(|_| local_error())
    }
    async fn delete_target(&self, id: &str) -> CoreResult<()> {
        self.0.delete(id).map_err(|_| local_error())
    }
}
struct Inventory(HelixStore);
#[async_trait::async_trait]
impl engram_integration::ScopeInventory for Inventory {
    async fn inventory(
        &self,
    ) -> CoreResult<Vec<engram_integration::scope_inventory::PartitionSummary>> {
        let memory = self
            .0
            .count_memory_partitions()?
            .into_iter()
            .map(|r| (r.scope, r.count))
            .collect::<Vec<_>>();
        let entity = self
            .0
            .count_entity_partitions()?
            .into_iter()
            .map(|r| (r.scope, r.count))
            .collect::<Vec<_>>();
        let belief = self
            .0
            .count_belief_partitions()?
            .into_iter()
            .map(|r| (r.scope, r.count))
            .collect::<Vec<_>>();
        Ok(engram_integration::scope_inventory::merge_partition_counts(
            &memory, &entity, &belief,
        ))
    }
    async fn partition_memories(
        &self,
        scope: &Scope,
        page: engram_integration::scope_inventory::PartitionPage,
    ) -> CoreResult<engram_integration::scope_inventory::PartitionRecords<MemoryRecord>> {
        page.validate()?;
        let items = self.0.partition_memories(scope, page.limit, page.offset)?;
        let next_offset = engram_integration::scope_inventory::next_page_offset(&items, &page);
        Ok(engram_integration::scope_inventory::PartitionRecords { items, next_offset })
    }
    async fn partition_entities(
        &self,
        scope: &Scope,
        page: engram_integration::scope_inventory::PartitionPage,
    ) -> CoreResult<engram_integration::scope_inventory::PartitionRecords<KnowledgeEntity>> {
        page.validate()?;
        let items = self.0.partition_entities(scope, page.limit, page.offset)?;
        let next_offset = engram_integration::scope_inventory::next_page_offset(&items, &page);
        Ok(engram_integration::scope_inventory::PartitionRecords { items, next_offset })
    }
    async fn partition_relationships(
        &self,
        scope: &Scope,
        page: engram_integration::scope_inventory::PartitionPage,
    ) -> CoreResult<engram_integration::scope_inventory::PartitionRecords<KnowledgeRelationship>>
    {
        page.validate()?;
        let items = self
            .0
            .partition_relationships(scope, page.limit, page.offset)?;
        let next_offset = engram_integration::scope_inventory::next_page_offset(&items, &page);
        Ok(engram_integration::scope_inventory::PartitionRecords { items, next_offset })
    }
    async fn partition_recent_memories(
        &self,
        scope: &Scope,
        limit: usize,
    ) -> CoreResult<Vec<MemoryRecord>> {
        engram_integration::scope_inventory::validate_recent_limit(limit)?;
        self.0.partition_recent_memories(scope, limit)
    }
}

struct QueryProvider(Arc<dyn engram_integration::EmbeddingProvider>);
impl engram_store_helixdb::retrieval::QueryVectorProvider for QueryProvider {
    fn query_vector(&self, request: &RetrievalRequest) -> CoreResult<(EmbeddingSpace, Vec<f32>)> {
        Ok((
            self.0.embedding_space(),
            self.0.embed_query(&request.query)?,
        ))
    }
}
#[cfg(feature = "embedded")]
pub fn open_embedded(
    storage: engram_store_helixdb::config::EmbeddedConfig,
    partition: engram_store_helixdb::codec::Partition,
    config: &EngramConfig,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    let store = HelixStore::open_embedded(storage, partition)
        .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
    finish_open(store, config, authorizer)
}
#[cfg(feature = "service")]
pub fn open_service(
    storage: engram_store_helixdb::config::ServiceConfig,
    partition: engram_store_helixdb::codec::Partition,
    config: &EngramConfig,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    let store = HelixStore::open_service(storage, partition)
        .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
    finish_open(store, config, authorizer)
}
#[cfg(any(feature = "embedded", feature = "service"))]
fn finish_open(
    store: HelixStore,
    config: &EngramConfig,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    match compose(store.clone(), config, authorizer) {
        Ok(provider) => Ok(provider),
        Err(error) => {
            store
                .close()
                .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
            Err(error)
        }
    }
}
