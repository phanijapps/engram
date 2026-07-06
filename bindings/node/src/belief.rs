//! Belief and contradiction engine for Node-API bridge.
//!
//! Stateful local belief engine exposed to Node through N-API.
//! Owns one SQLite-backed `SqlBeliefStore` so belief and contradiction calls
//! observe the same scoped state.

use engram_core::{BeliefRepository, ContradictionDetector};
use engram_domain::{Belief, Contradiction, ContradictionResolution, Id, Scope};
use engram_store_belief_sqlite::SqlBeliefStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stateful local belief engine exposed to Node through N-API.
///
/// Owns one SQLite-backed `SqlBeliefStore` so belief and contradiction calls
/// observe the same scoped state.
#[napi]
pub struct NativeBeliefEngine {
    store: SqlBeliefStore,
}

#[napi]
impl NativeBeliefEngine {
    /// Opens a SQLite belief engine. Pass a path for a durable file-backed
    /// store (shared with other engines that use the same file); omit for
    /// in-memory.
    #[napi(constructor)]
    pub fn new(path: Option<String>) -> Result<Self> {
        let store = match path {
            Some(path) => SqlBeliefStore::open_file(path),
            None => SqlBeliefStore::open_in_memory(),
        }
        .map_err(to_napi_error)?;
        Ok(Self { store })
    }

    /// Stores or updates a belief.
    #[napi(js_name = "putBeliefJson")]
    pub fn put_belief_json(&self, belief_json: String) -> Result<String> {
        let belief: Belief = decode(&belief_json)?;
        let result = block_on(self.store.put_belief(belief)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists beliefs in the given scope.
    #[napi(js_name = "listBeliefsJson")]
    pub fn list_beliefs_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_beliefs(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Stores or updates a contradiction record.
    #[napi(js_name = "putContradictionJson")]
    pub fn put_contradiction_json(&self, contradiction_json: String) -> Result<String> {
        let contradiction: Contradiction = decode(&contradiction_json)?;
        let result =
            block_on(self.store.put_contradiction(contradiction)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists contradictions in the given scope.
    #[napi(js_name = "listContradictionsJson")]
    pub fn list_contradictions_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.list_contradictions(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Retrieves a contradiction by ID and scope.
    #[napi(js_name = "getContradictionJson")]
    pub fn get_contradiction_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.store.get_contradiction(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Records a resolution for a contradiction.
    #[napi(js_name = "resolveContradictionJson")]
    pub fn resolve_contradiction_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let resolution: ContradictionResolution = serde_json::from_value(
            value
                .get("resolution")
                .cloned()
                .ok_or_else(|| Error::from_reason("missing 'resolution' field"))?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;
        let result = block_on(self.store.resolve_contradiction(&id, &scope, resolution))
            .map_err(to_napi_error)?;
        encode(&result)
    }

    /// Detects contradictions for a JSON array of `Belief`.
    #[napi(js_name = "detectContradictionsJson")]
    pub fn detect_contradictions_json(&self, beliefs_json: String) -> Result<String> {
        let beliefs = decode::<Vec<Belief>>(&beliefs_json)?;
        let result = block_on(self.store.detect_contradictions(&beliefs)).map_err(to_napi_error)?;
        encode(&result)
    }
}
