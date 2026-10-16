//! Offline input ablation probe. Queries and documents only; no relevance labels.
use engram_rerank_cross_encoder::{LocalOnnxScorer, RerankScorer};
use engram_retrieval::CodeIntelligenceConfig;
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CodeIntelligenceConfig::default().local_scorer;
    let scorer = LocalOnnxScorer::open_default(&config)?;
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let answer = (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let value: serde_json::Value = serde_json::from_str(&line?)?;
            let query = value["query"].as_str().ok_or("missing query")?;
            let documents: Vec<String> = serde_json::from_value(value["documents"].clone())?;
            let coverage = documents
                .iter()
                .map(|document| scorer.inspect_pair(query, document, 0))
                .collect::<Result<Vec<_>, _>>()?;
            let scores = scorer.score_batch(query, &documents, config.batch_size as usize)?;
            Ok(
                serde_json::json!({"scores": scores, "coverage": coverage, "identity": scorer.identity()}),
            )
        })();
        let value = match answer {
            Ok(value) => value,
            Err(error) => serde_json::json!({"error": error.to_string()}),
        };
        writeln!(stdout, "{value}")?;
        stdout.flush()?;
    }
    Ok(())
}
