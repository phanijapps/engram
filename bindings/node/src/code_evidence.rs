//! Bound native transport over the shared evidence operation.
use crate::{decode, encode, to_napi_error};
use engram_domain::{CodeEvidenceQuery, CodeSourceContext};
use engram_integration::EngramProvider;
use engram_runtime::CoreError;
use futures::executor::block_on;
use napi::Result;
use napi_derive::napi;
use std::sync::Arc;

#[napi]
pub struct NativeCodeEvidenceApi {
    provider: Arc<EngramProvider>,
    context: CodeSourceContext,
}
impl NativeCodeEvidenceApi {
    pub(crate) fn new(provider: Arc<EngramProvider>, context_json: &str) -> Result<Self> {
        let context: CodeSourceContext = decode(context_json)?;
        if context.reader_version != 1
            || context.scope.tenant.trim().is_empty()
            || context.requester.actor.id.as_str().trim().is_empty()
        {
            return Err(to_napi_error(CoreError::InvalidRequest {
                reason: "invalid trusted code evidence context".into(),
            }));
        }
        if provider.code_sources().is_none() || provider.code_segments().is_none() {
            return Err(to_napi_error(CoreError::CapabilityUnsupported {
                capability: "code_evidence".into(),
                reason: "source and segment authority unavailable".into(),
            }));
        }
        Ok(Self { provider, context })
    }
}
#[napi]
impl NativeCodeEvidenceApi {
    #[napi(js_name = "evidenceJson")]
    pub fn evidence_json(&self, query_json: String) -> Result<String> {
        let query: CodeEvidenceQuery = decode(&query_json)?;
        let packet = block_on(self.provider.code_evidence(
            &query,
            &self.context.scope,
            &self.context.requester,
        ))
        .map_err(to_napi_error)?;
        encode(&packet)
    }
}
