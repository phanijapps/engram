//! JSONL evaluation probe over the real provider's vector index and fused recall.
//! Run with `--features fastembed`; arguments are one complete EngramConfig file.
use std::io::{self, BufRead, Write};
use std::time::Instant;

use engram_domain::RetrievalRequest;
use engram_integration::{EngramConfig, EngramProvider};
use futures::executor::block_on;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::args().nth(1).ok_or("expected config path")?;
    let config: EngramConfig = serde_json::from_slice(&std::fs::read(config_path)?)?;
    let provider = EngramProvider::open(&config)?;
    let embedder = provider
        .embedding_provider()
        .ok_or("embedding provider unavailable")?;
    let vectors = provider.require_vectors()?;
    let recall = provider.require_recall()?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        let request: RetrievalRequest = serde_json::from_value(value)?;
        let start = Instant::now();
        let query = embedder.embed_query(&request.query)?;
        // SQLite returns raw distances in this port scalar. Evaluate hit order;
        // do not compare these values across different embedding models.
        let raw = block_on(vectors.search(
            &embedder.embedding_space(),
            query,
            request.limit.unwrap_or(10) as usize,
        ))?;
        let vector_seconds = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let fused = block_on(recall.recall(request))?;
        writeln!(
            out,
            "{}",
            json!({
                "space": embedder.embedding_space(),
                "vector": raw.into_iter().map(|(id,score)|json!({"targetId":id,"score":score})).collect::<Vec<_>>(),
                "fused": fused,
                "vectorSeconds": vector_seconds,
                "fusedSeconds": start.elapsed().as_secs_f64(),
            })
        )?;
        out.flush()?;
    }
    Ok(())
}
