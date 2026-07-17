//! Memory engine for Node-API bridge.
//!
//! Stateful local memory engine exposed to Node through N-API.
//! Each instance owns one SQLite-backed Rust service so write, retrieve, and
//! forget calls observe the same local state without TypeScript reimplementing
//! memory behavior.

use engram_domain::{ForgetRequest, RetrievalRequest, WriteMemoryRequest};
use engram_memory::MemoryService;
use engram_store_sqlite::SqlMemoryService;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{decode, encode, to_napi_error};

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
    pub fn new(path: Option<String>) -> Result<Self> {
        let service = match path {
            Some(path) => SqlMemoryService::open_file(path),
            None => SqlMemoryService::open_in_memory(),
        }
        .map_err(to_napi_error)?;
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
