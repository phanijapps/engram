use engram_runtime::CoreResult;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::{HttpEmbeddingConfig, HttpEmbeddingProtocol};
use crate::provider::failed;

pub(crate) fn request(config: &HttpEmbeddingConfig, inputs: &[String]) -> Value {
    let mut body = json!({"model":config.model,"input":inputs});
    match config.protocol {
        HttpEmbeddingProtocol::Ollama => body["truncate"] = json!(false),
        HttpEmbeddingProtocol::OpenAiCompatible => body["encoding_format"] = json!("float"),
    }
    if let Some(dimensions) = config.dimensions {
        body["dimensions"] = json!(dimensions);
    }
    body
}

#[derive(Deserialize)]
struct OllamaResponse {
    embeddings: Vec<Vec<f32>>,
}
#[derive(Deserialize)]
struct OpenAiResponse {
    data: Vec<IndexedEmbedding>,
}
#[derive(Deserialize)]
struct IndexedEmbedding {
    index: usize,
    embedding: Vec<f32>,
}

pub(crate) fn response(
    protocol: HttpEmbeddingProtocol,
    bytes: &[u8],
    count: usize,
) -> CoreResult<Vec<Vec<f32>>> {
    let vectors = match protocol {
        HttpEmbeddingProtocol::Ollama => serde_json::from_slice::<OllamaResponse>(bytes)
            .map(|v| v.embeddings)
            .map_err(|_| failed("invalid embedding response"))?,
        HttpEmbeddingProtocol::OpenAiCompatible => {
            let mut parsed: OpenAiResponse =
                serde_json::from_slice(bytes).map_err(|_| failed("invalid embedding response"))?;
            parsed.data.sort_by_key(|v| v.index);
            if parsed.data.len() != count
                || parsed
                    .data
                    .iter()
                    .enumerate()
                    .any(|(index, v)| index != v.index)
            {
                return Err(failed("embedding response indices do not match inputs"));
            }
            parsed.data.into_iter().map(|v| v.embedding).collect()
        }
    };
    if vectors.len() != count {
        return Err(failed("embedding response count does not match inputs"));
    }
    Ok(vectors)
}
