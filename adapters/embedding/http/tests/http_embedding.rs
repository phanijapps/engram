use engram_embedding_http::{HttpEmbeddingConfig, HttpEmbeddingProtocol, HttpEmbeddingProvider};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

fn config(endpoint: String, protocol: HttpEmbeddingProtocol) -> HttpEmbeddingConfig {
    HttpEmbeddingConfig {
        protocol,
        model: "fixture-alias".into(),
        endpoint,
        dimensions: None,
        api_key_env: None,
        query_instruction: Some("query: ".into()),
        passage_instruction: Some("passage: ".into()),
        timeout_secs: 2,
        batch_size: 2,
    }
}

fn server(
    responses: Vec<(u16, String)>,
) -> (
    String,
    mpsc::Receiver<(String, Value)>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut b = [0u8; 1024];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let len: usize = header
                .lines()
                .find_map(|v| {
                    v.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < header_end + len {
                let mut b = [0u8; 1024];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            let _ = tx.send((
                header,
                serde_json::from_slice(&bytes[header_end..header_end + len]).unwrap(),
            ));
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (endpoint, rx, handle)
}
fn ok(value: Value) -> (u16, String) {
    (200, value.to_string())
}
fn ollama(v: Vec<Vec<f32>>) -> (u16, String) {
    ok(json!({"model":"server-model-alias","embeddings":v}))
}
fn error(config: HttpEmbeddingConfig) -> String {
    match HttpEmbeddingProvider::new(config) {
        Ok(_) => panic!("expected failure"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn ollama_infers_dimensions_and_formats_requests_and_batches() {
    let (url, rx, handle) = server(vec![
        ollama(vec![vec![1., 0., 0.]]),
        ollama(vec![vec![0., 1., 0.]]),
        ollama(vec![vec![1., 0., 0.], vec![0., 1., 0.]]),
        ollama(vec![vec![0., 0., 1.]]),
    ]);
    let provider = HttpEmbeddingProvider::new(config(url, HttpEmbeddingProtocol::Ollama)).unwrap();
    assert_eq!(provider.dimensions(), 3);
    assert_eq!(
        provider.embed_query(" question ").unwrap(),
        vec![0., 1., 0.]
    );
    assert_eq!(
        provider
            .embed_batch(&["one".into(), "two".into(), "three".into()])
            .unwrap()
            .len(),
        3
    );
    handle.join().unwrap();
    let requests: Vec<_> = rx.iter().collect();
    assert!(requests[0].0.starts_with("POST /api/embed "));
    assert_eq!(requests[0].1["truncate"], false);
    assert!(requests[0].1.get("dimensions").is_none());
    assert_eq!(requests[1].1["input"], json!(["query: question"]));
    assert_eq!(
        requests[2].1["input"],
        json!(["passage: one", "passage: two"])
    );
    assert_eq!(requests[3].1["input"], json!(["passage: three"]));
}

#[test]
fn openai_orders_indices_and_requests_explicit_dimensions() {
    let (url, rx, handle) = server(vec![
        ok(json!({"data":[{"index":0,"embedding":[1.,2.]}]})),
        ok(json!({"data":[{"index":1,"embedding":[3.,4.]},{"index":0,"embedding":[1.,2.]}]})),
    ]);
    let mut cfg = config(
        format!("{url}/v1/"),
        HttpEmbeddingProtocol::OpenAiCompatible,
    );
    cfg.dimensions = Some(2);
    let provider = HttpEmbeddingProvider::new(cfg).unwrap();
    assert_eq!(
        provider.embed_batch(&["a".into(), "b".into()]).unwrap(),
        vec![vec![1., 2.], vec![3., 4.]]
    );
    handle.join().unwrap();
    for (header, body) in rx.iter() {
        assert!(header.starts_with("POST /v1/embeddings "));
        assert_eq!(body["dimensions"], 2);
        assert_eq!(body["encoding_format"], "float");
    }
}

#[test]
fn rejects_vector_mismatches_malformed_counts_and_nonfinite_overflow() {
    for response in [
        json!({"embeddings":[[]]}).to_string(),
        json!({"embeddings":[]}).to_string(),
        "{\"embeddings\":[[1e100]]}".into(),
        "sensitive malformed body".into(),
    ] {
        let (url, _, handle) = server(vec![(200, response)]);
        assert!(!error(config(url, HttpEmbeddingProtocol::Ollama)).contains("sensitive"));
        handle.join().unwrap();
    }
    let (url, _, handle) = server(vec![ollama(vec![vec![1., 2.]])]);
    let mut cfg = config(url, HttpEmbeddingProtocol::Ollama);
    cfg.dimensions = Some(3);
    assert!(error(cfg).contains("invalid vectors or dimensions"));
    handle.join().unwrap();
}

#[test]
fn rejects_duplicate_or_missing_openai_indices() {
    for data in [
        json!([{"index":1,"embedding":[1.]}]),
        json!([{"index":0,"embedding":[1.]},{"index":0,"embedding":[2.]}]),
    ] {
        let (url, _, handle) = server(vec![ok(json!({"data":data}))]);
        assert!(error(config(url, HttpEmbeddingProtocol::OpenAiCompatible)).contains("indices"));
        handle.join().unwrap();
    }
}

#[test]
fn validates_endpoint_options_before_any_request() {
    for url in [
        "http://example.com",
        "ftp://localhost",
        "https://secret:token@example.com",
        "https://example.com?a=secret",
        "https://example.com#secret",
    ] {
        let message = error(config(url.into(), HttpEmbeddingProtocol::Ollama));
        assert!(!message.contains("secret"));
        assert!(!message.contains("token"));
    }
    let mut cfg = config("http://127.0.0.1:1".into(), HttpEmbeddingProtocol::Ollama);
    cfg.dimensions = Some(8193);
    assert!(error(cfg).contains("dimensions"));
    let mut cfg = config("http://127.0.0.1:1".into(), HttpEmbeddingProtocol::Ollama);
    cfg.batch_size = 0;
    assert!(error(cfg).contains("batch"));
    let mut cfg = config("http://127.0.0.1:1".into(), HttpEmbeddingProtocol::Ollama);
    cfg.api_key_env = Some("INVALID-NAME".into());
    assert!(error(cfg).contains("environment variable name"));
}

#[test]
fn redacts_server_errors_and_does_not_follow_redirects() {
    for status in [401, 500, 302] {
        let (url, _, handle) = server(vec![(status, "secret key and private input".into())]);
        let message = error(config(url, HttpEmbeddingProtocol::Ollama));
        assert!(!message.contains("secret"));
        assert!(!message.contains("127.0.0.1"));
        handle.join().unwrap();
    }
}

#[test]
fn rejects_dimension_changes_after_probe_and_empty_or_oversized_inputs() {
    let (url, _, handle) = server(vec![ollama(vec![vec![1., 2.]]), ollama(vec![vec![1.]])]);
    let mut cfg = config(url, HttpEmbeddingProtocol::Ollama);
    cfg.passage_instruction = None;
    let provider = HttpEmbeddingProvider::new(cfg).unwrap();
    assert!(provider.embed_passage(" ").is_err());
    assert!(provider.embed_query(&"x".repeat(1024 * 1024 + 1)).is_err());
    assert!(provider.embed_passage("actual input").is_err());
    handle.join().unwrap();
}

#[test]
fn limits_response_body() {
    let (url, _, handle) = server(vec![(200, "x".repeat(32 * 1024 * 1024 + 1))]);
    assert!(error(config(url, HttpEmbeddingProtocol::Ollama)).contains("size limit"));
    // The client closes after Content-Length validation, so the fixture's write may fail.
    let _ = handle.join();
}

#[test]
fn request_timeout_is_bounded_and_redacted() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut cfg = config(
        format!("http://{}", listener.local_addr().unwrap()),
        HttpEmbeddingProtocol::Ollama,
    );
    cfg.timeout_secs = 1;
    let handle = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(1400));
    });
    let start = std::time::Instant::now();
    let message = error(cfg);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(
        message,
        "adapter failed: engram-embedding-http: embedding HTTP request failed"
    );
    handle.join().unwrap();
}

#[test]
fn redirect_cannot_forward_requests_to_another_endpoint() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let location = format!("http://{}", target.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut b = [0u8; 2048];
        let _ = stream.read(&mut b);
        write!(stream,"HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert!(error(config(endpoint, HttpEmbeddingProtocol::Ollama)).contains("unsuccessful status"));
    handle.join().unwrap();
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
