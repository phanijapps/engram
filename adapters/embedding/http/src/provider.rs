use std::{io::Read, time::Duration};

use engram_runtime::{CoreError, CoreResult};
use reqwest::{
    Url,
    blocking::Client,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};

use crate::{
    config::{HttpEmbeddingConfig, invalid},
    protocol,
};

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn failed(message: &'static str) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-embedding-http".into(),
        message: message.into(),
    }
}

/// Inference adapter with a probed, immutable output dimension.
pub struct HttpEmbeddingProvider {
    client: Client,
    url: Url,
    authorization: Option<HeaderValue>,
    config: HttpEmbeddingConfig,
    dimensions: u32,
}

impl HttpEmbeddingProvider {
    pub fn new(config: HttpEmbeddingConfig) -> CoreResult<Self> {
        let url = config.url()?;
        let authorization = config
            .api_key_env
            .as_ref()
            .map(|name| {
                if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(invalid(
                        "invalid embedding API key environment variable name",
                    ));
                }
                let key = std::env::var(name).map_err(|_| {
                    invalid("embedding API key environment variable is unavailable")
                })?;
                if key.trim().is_empty() {
                    return Err(invalid("embedding API key is empty"));
                }
                let mut header = HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| invalid("invalid embedding API key"))?;
                header.set_sensitive(true);
                Ok(header)
            })
            .transpose()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(config.timeout_secs.min(10)))
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| failed("could not initialize embedding HTTP client"))?;
        let mut provider = Self {
            client,
            url,
            authorization,
            dimensions: config.dimensions.unwrap_or(0),
            config,
        };
        let probe = provider.embed_passage("embedding dimension probe")?;
        provider.dimensions = probe.len() as u32;
        Ok(provider)
    }

    pub fn dimensions(&self) -> u32 {
        self.dimensions
    }

    pub fn embed_query(&self, text: &str) -> CoreResult<Vec<f32>> {
        let input = self.input(text, self.config.query_instruction.as_deref())?;
        Ok(self.send(&[input])?.remove(0))
    }

    pub fn embed_passage(&self, text: &str) -> CoreResult<Vec<f32>> {
        let input = self.input(text, self.config.passage_instruction.as_deref())?;
        Ok(self.send(&[input])?.remove(0))
    }

    pub fn embed_batch(&self, texts: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        // Validate all inputs before making a partial batch request.
        if texts.len() > 4096 {
            return Err(invalid("embedding call exceeds 4096 inputs"));
        }
        if texts
            .iter()
            .try_fold(0usize, |total, text| total.checked_add(text.len()))
            .is_none_or(|total| total > 16 * 1024 * 1024)
        {
            return Err(invalid("embedding call exceeds sixteen MiB of input"));
        }
        let inputs = texts
            .iter()
            .map(|text| self.input(text, self.config.passage_instruction.as_deref()))
            .collect::<CoreResult<Vec<_>>>()?;
        let mut result = Vec::with_capacity(inputs.len());
        for batch in inputs.chunks(self.config.batch_size) {
            result.extend(self.send(batch)?);
        }
        Ok(result)
    }

    fn input(&self, text: &str, instruction: Option<&str>) -> CoreResult<String> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(invalid("embedding input exceeds one MiB"));
        }
        if text.trim().is_empty() {
            return Err(invalid("embedding input must not be empty"));
        }
        let input = format!("{}{}", instruction.unwrap_or_default(), text.trim());
        Ok(input)
    }

    fn send(&self, inputs: &[String]) -> CoreResult<Vec<Vec<f32>>> {
        let body = serde_json::to_vec(&protocol::request(&self.config, inputs))
            .map_err(|_| failed("could not encode embedding request"))?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(invalid("embedding request exceeds four MiB"));
        }
        let mut request = self
            .client
            .post(self.url.clone())
            .header("content-type", "application/json")
            .body(body);
        if let Some(value) = &self.authorization {
            request = request.header(AUTHORIZATION, value);
        }
        let response = request
            .send()
            .map_err(|_| failed("embedding HTTP request failed"))?;
        if !response.status().is_success() {
            return Err(failed("embedding server returned an unsuccessful status"));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES)
        {
            return Err(failed("embedding response exceeds size limit"));
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failed("could not read embedding response"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(failed("embedding response exceeds size limit"));
        }
        let vectors = protocol::response(self.config.protocol, &bytes, inputs.len())?;
        let dimensions = if self.dimensions == 0 {
            vectors.first().map_or(0, Vec::len)
        } else {
            self.dimensions as usize
        };
        if !(1..=8192).contains(&dimensions)
            || vectors
                .iter()
                .any(|v| v.len() != dimensions || v.iter().any(|n| !n.is_finite()))
        {
            return Err(failed(
                "embedding response contains invalid vectors or dimensions",
            ));
        }
        Ok(vectors)
    }
}
