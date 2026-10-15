#![allow(dead_code)]
use async_trait::async_trait;
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_integration::{
    CapabilityReport, EmbeddingProvider, EngramProvider, EngramProviderBuilder,
    code_representation::code_representation,
};
use engram_knowledge::KnowledgeGraphRepository;
use engram_retrieval::{
    CodeIntelligenceConfig, CodeRetrievalLane, RetrievalIndex, TextTokenization,
};
use engram_runtime::CoreResult;
use engram_store_sqlite::{SqlKnowledgeStore, SqliteVectorIndex};
use futures::executor::block_on;
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub(crate) struct Model {
    pub(crate) queries: AtomicUsize,
}
impl EmbeddingProvider for Model {
    fn is_local(&self) -> bool {
        true
    }
    fn provider_id(&self) -> &str {
        "local-test"
    }
    fn model_id(&self) -> &str {
        "scalar"
    }
    fn dimensions(&self) -> u32 {
        3
    }
    fn embedding_space(&self) -> EmbeddingSpace {
        EmbeddingSpace::new("local-test", "scalar", 3, "passage", None::<String>)
    }
    fn tokenize_untruncated(&self, text: &str, specials: bool) -> CoreResult<TextTokenization> {
        Ok(TextTokenization {
            offsets: text
                .char_indices()
                .map(|(i, c)| CodeByteRange {
                    start: i as u64,
                    end: (i + c.len_utf8()) as u64,
                })
                .collect(),
            special_tokens: if specials { 2 } else { 0 },
            window: 512,
            tokenizer_id: "scalar-v1".into(),
        })
    }
    fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
        panic!("code must use prepared query")
    }
    fn embed_prepared_code_query(
        &self,
        _: &engram_integration::code_query::PreparedCodeQuery,
    ) -> CoreResult<Vec<f32>> {
        self.queries.fetch_add(1, Ordering::SeqCst);
        Ok(vec![1., 0., 0.])
    }
    fn embed_passage(&self, _: &str) -> CoreResult<Vec<f32>> {
        Ok(vec![1., 0., 0.])
    }
}
pub(crate) struct Lane {
    pub(crate) rows: Vec<RetrievalResult>,
    pub(crate) calls: Mutex<Vec<RetrievalRequest>>,
}
#[async_trait]
impl RetrievalIndex for Lane {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        self.calls.lock().unwrap().push(request.clone());
        Ok(self
            .rows
            .iter()
            .take(request.limit.unwrap() as usize)
            .cloned()
            .collect())
    }
}
pub(crate) struct Graph {
    pub(crate) store: Arc<SqlKnowledgeStore>,
    pub(crate) calls: Mutex<Vec<u32>>,
}
#[async_trait]
impl KnowledgeGraphRepository for Graph {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        self.store.put_graph(graph).await
    }
    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        self.store.get_graph(id, scope).await
    }
    async fn neighbors(
        &self,
        graph: &KnowledgeGraphId,
        node: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.calls.lock().unwrap().push(limit.unwrap());
        self.store.neighbors(graph, node, scope, limit).await
    }
}
pub(crate) struct Fixture {
    pub(crate) _root: tempfile::TempDir,
    pub(crate) store: Arc<SqlKnowledgeStore>,
    pub(crate) provider: EngramProvider,
    pub(crate) scope: Scope,
    pub(crate) requester: Requester,
    pub(crate) model: Arc<Model>,
    pub(crate) lexical: Arc<Lane>,
    pub(crate) graph: Arc<Lane>,
    pub(crate) config: CodeIntelligenceConfig,
    pub(crate) expansion: Arc<Graph>,
}
pub(crate) fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let text: String = (0..35)
        .map(|i| {
            format!(
                "export function run{i}() {{ return run{}(); }}\n",
                (i + 1) % 35
            )
        })
        .collect();
    std::fs::write(root.path().join("public.ts"), text).unwrap();
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let scope: Scope = serde_json::from_value(json!({"tenant":"t","workspace":"w"})).unwrap();
    let requester: Requester =
        serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap();
    let options = ScanOptions {
        scope: scope.clone(),
        policy: serde_json::from_value(
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
        )
        .unwrap(),
        actor: requester.actor.clone(),
        source_name: "fixture".into(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: Default::default(),
    };
    assert_eq!(
        scan_repository(root.path(), &options, store.as_ref(), |_| {})
            .unwrap()
            .0
            .errors,
        0
    );
    let private_root = tempfile::tempdir().unwrap();
    std::fs::write(
        private_root.path().join("secret.ts"),
        "export function secretOnly() { return 'PRIVATE_MARKER'; }\n",
    )
    .unwrap();
    let mut private_options = options.clone();
    private_options.policy.visibility = Visibility::Private;
    private_options.actor.id = "private-owner".into();
    private_options.source_name = "private-fixture".into();
    assert_eq!(
        scan_repository(
            private_root.path(),
            &private_options,
            store.as_ref(),
            |_| {}
        )
        .unwrap()
        .0
        .errors,
        0
    );
    let chunks = block_on(store.list_chunks(&scope)).unwrap();
    let rows = chunks
        .into_iter()
        .map(|parent| RetrievalResult {
            id: parent.id.to_string(),
            target_id: parent.id.to_string(),
            target_type: RetrievalTargetType::Chunk,
            content: parent.text,
            score: RetrievalScore {
                total: 1.,
                relevance: None,
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: None,
            },
            provenance: parent.provenance,
            policy: parent.policy,
            explanation: None,
            fusion_trace: None,
            metadata: None,
        })
        .collect();
    let lexical = Arc::new(Lane {
        rows,
        calls: Mutex::new(vec![]),
    });
    let graph = Arc::new(Lane {
        rows: vec![],
        calls: Mutex::new(vec![]),
    });
    let expansion = Arc::new(Graph {
        store: store.clone(),
        calls: Mutex::new(vec![]),
    });
    let model = Arc::new(Model {
        queries: AtomicUsize::new(0),
    });
    let config = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1);
    let rep = code_representation(model.as_ref(), &config.index).unwrap();
    let base = Arc::new(
        SqliteVectorIndex::open_in_memory(3)
            .unwrap()
            .with_embedding_space(model.embedding_space()),
    );
    let code = Arc::new(
        SqliteVectorIndex::open_in_memory(3)
            .unwrap()
            .with_embedding_space(rep.embedding_space),
    );
    let provider = EngramProviderBuilder::new(CapabilityReport::new(CapabilityState::Supported))
        .knowledge(store.clone())
        .knowledge_query(store.clone())
        .graph(expansion.clone())
        .embedding_provider(model.clone())
        .vectors(base)
        .code_vectors(code)
        .code_retrieval_lane(CodeRetrievalLane::Lexical, lexical.clone())
        .code_retrieval_lane(CodeRetrievalLane::Graph, graph.clone())
        .build();
    let result = block_on(provider.backfill_embeddings(&EmbeddingBackfillRequest {
        scope: scope.clone(),
        requester: requester.clone(),
        limit: 1024,
        source_filter: None,
    }))
    .unwrap();
    assert_eq!(result.remaining, 0);
    Fixture {
        _root: root,
        store,
        provider,
        scope,
        requester,
        model,
        lexical,
        graph,
        config,
        expansion,
    }
}
pub(crate) fn query() -> CodeEvidenceQuery {
    serde_json::from_value(json!({"query":"runs a numbered operation","resultLimit":10})).unwrap()
}
