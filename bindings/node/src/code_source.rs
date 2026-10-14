//! Bound source API: the embedding host supplies principal/scope at construction.
use crate::{decode, encode, to_napi_error};
use engram_domain::{CodeSourceContext, CodeSourceQuery};
use engram_knowledge::KnowledgeRepository;
use futures::executor::block_on;
use napi::Result;
use napi_derive::napi;
use std::sync::Arc;
#[napi]
pub struct NativeCodeSourceApi {
    handle: Arc<dyn KnowledgeRepository>,
    context: CodeSourceContext,
}
impl NativeCodeSourceApi {
    pub(crate) fn new(handle: Arc<dyn KnowledgeRepository>, context_json: &str) -> Result<Self> {
        let context: CodeSourceContext = decode(context_json)?;
        if context.reader_version != 1
            || context.scope.tenant.is_empty()
            || handle.code_sources().is_none()
        {
            return Err(to_napi_error(
                engram_runtime::CoreError::CapabilityUnsupported {
                    capability: "code_source_preservation".into(),
                    reason: "source cell or reader version unsupported".into(),
                },
            ));
        }
        Ok(Self { handle, context })
    }
}
#[napi]
impl NativeCodeSourceApi {
    #[napi(js_name = "readSourceJson")]
    pub fn read_source_json(&self, query_json: String) -> Result<String> {
        let query: CodeSourceQuery = decode(&query_json)?;
        let port = self.handle.code_sources().unwrap();
        encode(
            &block_on(port.read_code_source(self.context.request(query), &self.context.scope))
                .map_err(to_napi_error)?,
        )
    }
    #[napi(js_name = "listSourcesJson")]
    pub fn list_sources_json(&self) -> Result<String> {
        let port = self.handle.code_sources().unwrap();
        // Listing requires the same per-source policy as reading; no unauthenticated
        // path/status inventory can be enumerated through this handle.
        let sources =
            block_on(port.list_code_sources(&self.context.scope)).map_err(to_napi_error)?;
        let mut visible = Vec::new();
        for m in sources {
            let request = self.context.request(CodeSourceQuery {
                repository: m.file.repository.clone(),
                path: m.file.path.clone(),
                generation: None,
                max_bytes: u64::MAX,
            });
            if engram_knowledge::authorize_code_source_access(
                &request,
                &self.context.scope,
                &m,
                chrono::Utc::now(),
            )
            .is_ok()
            {
                visible.push(m);
            }
        }
        encode(&visible)
    }
}
