//! Node-API bridge for Engram memory operations.
//!
//! The binding is intentionally a JSON transport over Rust behavior. TypeScript
//! packages own ergonomics; this crate owns serialization round trips into the
//! Rust memory service.

use engram_core::MemoryService;
use engram_domain::{ForgetRequest, RetrievalRequest, WriteMemoryRequest};
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Stateful local memory engine exposed to Node through N-API.
///
/// Each instance owns one SQLite-backed Rust service so write, retrieve, and
/// forget calls observe the same local state without TypeScript reimplementing
/// memory behavior.
#[napi]
pub struct NativeMemoryEngine {
    service: SqlMemoryService,
}

#[napi]
impl NativeMemoryEngine {
    /// Opens a local in-memory SQLite engine for Node consumers and tests.
    ///
    /// The database is process-local to the native engine instance. Durable
    /// file-backed configuration should be added through explicit adapter
    /// options rather than inferred from JavaScript global state.
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let service = SqlMemoryService::open_in_memory().map_err(to_napi_error)?;
        Ok(Self { service })
    }

    /// Writes one memory using a JSON-encoded v1 `WriteMemoryRequest`.
    ///
    /// The returned string is a JSON-encoded v1 `WriteMemoryResponse` produced
    /// by Rust service behavior.
    #[napi(js_name = "writeMemoryJson")]
    pub fn write_memory_json(&self, request_json: String) -> Result<String> {
        let request = decode::<WriteMemoryRequest>(&request_json)?;
        let response = block_on(self.service.write_memory(request)).map_err(to_napi_error)?;
        encode(&response)
    }

    /// Retrieves context using a JSON-encoded v1 `RetrievalRequest`.
    ///
    /// The binding returns the Rust service response unchanged as JSON so the
    /// TypeScript client can validate and compose it without reimplementing
    /// retrieval behavior.
    #[napi(js_name = "retrieveJson")]
    pub fn retrieve_json(&self, request_json: String) -> Result<String> {
        let request = decode::<RetrievalRequest>(&request_json)?;
        let response = block_on(self.service.retrieve(request)).map_err(to_napi_error)?;
        encode(&response)
    }

    /// Applies a forget operation using a JSON-encoded v1 `ForgetRequest`.
    ///
    /// Policy, scope, lifecycle status, and audit-event semantics are enforced
    /// by the Rust service behind the binding.
    #[napi(js_name = "forgetJson")]
    pub fn forget_json(&self, request_json: String) -> Result<String> {
        let request = decode::<ForgetRequest>(&request_json)?;
        let response = block_on(self.service.forget(request)).map_err(to_napi_error)?;
        encode(&response)
    }
}

fn decode<T>(json: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(json).map_err(|error| Error::from_reason(error.to_string()))
}

fn encode<T>(value: &T) -> Result<String>
where
    T: serde::Serialize,
{
    serde_json::to_string(value).map_err(|error| Error::from_reason(error.to_string()))
}

fn to_napi_error(error: engram_core::CoreError) -> Error {
    Error::from_reason(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{ContextPayload, ForgetResult, WriteMemoryResponse};

    fn write_fixture() -> String {
        include_str!("../../../contracts/v1/examples/write-memory-request.json").to_owned()
    }

    fn retrieval_fixture() -> String {
        include_str!("../../../contracts/v1/examples/retrieval-request.json").to_owned()
    }

    fn forget_request(memory_id: &str) -> String {
        format!(
            r#"{{
                "targetType": "memory",
                "targetId": "{memory_id}",
                "scope": {{
                    "tenant": "tenant-demo",
                    "workspace": "engram",
                    "environment": "local"
                }},
                "requester": {{
                    "actor": {{
                        "id": "actor-agent-1",
                        "kind": "agent",
                        "displayName": "Contract Agent"
                    }},
                    "roles": ["maintainer"],
                    "permissions": ["memory.forget"]
                }},
                "mode": "delete",
                "reason": "native bridge test"
            }}"#
        )
    }

    #[test]
    fn native_engine_round_trips_write_retrieve_and_forget_json() {
        let engine = NativeMemoryEngine::new().expect("engine");

        let write_response = engine
            .write_memory_json(write_fixture())
            .expect("write memory");
        let write_response =
            serde_json::from_str::<WriteMemoryResponse>(&write_response).expect("write response");

        let context = engine
            .retrieve_json(retrieval_fixture())
            .expect("retrieve context");
        let context = serde_json::from_str::<ContextPayload>(&context).expect("context");
        assert_eq!(context.items.len(), 1);

        let result = engine
            .forget_json(forget_request(&write_response.record.id.to_string()))
            .expect("forget memory");
        let result = serde_json::from_str::<ForgetResult>(&result).expect("forget result");
        assert_eq!(result.status, engram_domain::ForgetStatus::Deleted);
    }
}
