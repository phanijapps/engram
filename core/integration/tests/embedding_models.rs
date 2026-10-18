#![cfg(all(feature = "sqlite", feature = "http-embeddings"))]

use engram_domain::{CapabilityState, EmbeddingSpace, Id, ScopeMappingStrategy};
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationMode,
};
use futures::executor::block_on;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

struct ModelServer {
    endpoint: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl ModelServer {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("fixture accept: {error}"),
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let body = loop {
                    let mut buffer = [0; 4096];
                    let size = stream.read(&mut buffer).unwrap();
                    assert_ne!(size, 0);
                    bytes.extend_from_slice(&buffer[..size]);
                    if let Some(header_end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..header_end]);
                        let size: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= header_end + 4 + size {
                            break serde_json::from_slice::<serde_json::Value>(
                                &bytes[header_end + 4..header_end + 4 + size],
                            )
                            .unwrap();
                        }
                    }
                };
                let dimensions = if body["model"] == "three" { 3 } else { 2 };
                let count = body["input"].as_array().unwrap().len();
                let response =
                    serde_json::json!({"embeddings": vec![vec![1.0;dimensions];count]}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            }
        });
        Self {
            endpoint,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for ModelServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn config(root: &std::path::Path, endpoint: &str, model: &str, dimensions: u32) -> EngramConfig {
    let mut embedding = EmbeddingProviderConfig {
        provider_type: "ollama".into(),
        model: model.into(),
        dimensions,
        ..Default::default()
    };
    embedding.options.endpoint = Some(endpoint.into());
    EngramConfig::new(
        root.join("store"),
        root,
        ScopeMappingStrategy::Strict,
        embedding,
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
}

#[test]
fn sqlite_fences_same_size_model_and_dimension_changes_without_losing_vectors() {
    let root = tempfile::tempdir().unwrap();
    let server = ModelServer::new();
    let original = config(root.path(), &server.endpoint, "original", 2);
    let first = EngramProvider::open(&original).unwrap();
    let vectors = first.require_vectors().unwrap();
    let space: EmbeddingSpace = vectors.embedding_space().clone();
    block_on(vectors.insert(&Id::from("kept"), &space, vec![1.0, 2.0])).unwrap();
    drop(first);
    for (model, dimensions) in [("same-width-other-model", 2), ("three", 3)] {
        let changed =
            EngramProvider::open(&config(root.path(), &server.endpoint, model, dimensions))
                .unwrap();
        assert!(matches!(
            changed.capabilities().vectors,
            CapabilityState::RequiresReindex { .. }
        ));
        assert!(changed.vectors().is_none());
        assert!(changed.embedding_provider().is_none());
    }
    let mut instructed = original.clone();
    instructed.embedding_provider.options.query_instruction = Some("new instruction: ".into());
    let changed = EngramProvider::open(&instructed).unwrap();
    assert!(matches!(
        changed.capabilities().vectors,
        CapabilityState::RequiresReindex { .. }
    ));
    assert!(changed.vectors().is_none());
    drop(changed);
    let restored = EngramProvider::open(&original).unwrap();
    assert_eq!(
        block_on(
            restored
                .require_vectors()
                .unwrap()
                .vector_for_target(&Id::from("kept"))
        )
        .unwrap(),
        Some(vec![1.0, 2.0])
    );
}

#[test]
fn automatic_dimensions_and_normalization_reach_index_and_inference_together() {
    let root = tempfile::tempdir().unwrap();
    let server = ModelServer::new();
    let mut config = config(root.path(), &server.endpoint, "automatic", 0);
    config.embedding_provider.normalization = Some("l2".into());
    let provider = EngramProvider::open(&config).unwrap();
    let embedder = provider.require_embedding_provider().unwrap();
    assert_eq!(embedder.dimensions(), 2);
    assert_eq!(
        embedder.embedding_space(),
        *provider.require_vectors().unwrap().embedding_space()
    );
    let vector = embedder.embed_query("query").unwrap();
    assert!((vector.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-5);
}

#[test]
fn wrong_declared_dimensions_fail_before_any_vector_index_is_created() {
    let root = tempfile::tempdir().unwrap();
    let server = ModelServer::new();
    assert!(EngramProvider::open(&config(root.path(), &server.endpoint, "two", 3)).is_err());
    assert!(!root.path().join("store/vectors.db").exists());
}
