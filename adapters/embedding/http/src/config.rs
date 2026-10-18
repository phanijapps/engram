use std::net::IpAddr;

use engram_runtime::{CoreError, CoreResult};
use reqwest::Url;

#[derive(Clone, Copy, Debug)]
pub enum HttpEmbeddingProtocol {
    Ollama,
    OpenAiCompatible,
}

/// Credentials are read from the named environment variable, never embedded in config.
#[derive(Clone, Debug)]
pub struct HttpEmbeddingConfig {
    pub protocol: HttpEmbeddingProtocol,
    pub model: String,
    pub endpoint: String,
    pub dimensions: Option<u32>,
    pub api_key_env: Option<String>,
    pub query_instruction: Option<String>,
    pub passage_instruction: Option<String>,
    pub timeout_secs: u64,
    pub batch_size: usize,
}

pub(crate) fn invalid(reason: &'static str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}

impl HttpEmbeddingConfig {
    pub(crate) fn url(&self) -> CoreResult<Url> {
        if self.model.trim().is_empty() || self.model.len() > 1024 {
            return Err(invalid("embedding model must contain 1–1024 bytes"));
        }
        if self.dimensions.is_some_and(|d| d == 0 || d > 8192) {
            return Err(invalid("embedding dimensions must be between 1 and 8192"));
        }
        if !(1..=300).contains(&self.timeout_secs) || !(1..=256).contains(&self.batch_size) {
            return Err(invalid(
                "embedding timeout must be 1–300 seconds and batch size 1–256",
            ));
        }
        if [&self.query_instruction, &self.passage_instruction]
            .into_iter()
            .flatten()
            .any(|v| v.len() > 8192)
        {
            return Err(invalid("embedding instructions exceed 8192 bytes"));
        }
        let mut url =
            Url::parse(&self.endpoint).map_err(|_| invalid("invalid embedding endpoint URL"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid(
                "embedding endpoint cannot contain credentials, query or fragment",
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| invalid("embedding endpoint requires a host"))?;
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err(invalid(
                "embedding endpoint requires HTTPS or HTTP loopback",
            ));
        }
        let base = url.path().trim_end_matches('/');
        let path = match self.protocol {
            HttpEmbeddingProtocol::Ollama if base.ends_with("/api/embed") => base.to_owned(),
            HttpEmbeddingProtocol::Ollama if base.ends_with("/api") => format!("{base}/embed"),
            HttpEmbeddingProtocol::Ollama => format!("{base}/api/embed"),
            HttpEmbeddingProtocol::OpenAiCompatible if base.ends_with("/embeddings") => {
                base.to_owned()
            }
            HttpEmbeddingProtocol::OpenAiCompatible if base.ends_with("/v1") => {
                format!("{base}/embeddings")
            }
            HttpEmbeddingProtocol::OpenAiCompatible => format!("{base}/v1/embeddings"),
        };
        url.set_path(&path);
        Ok(url)
    }
}
