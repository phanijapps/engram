//! Consolidation planning engine for Node-API bridge.
//!
//! Stateless consolidation behavior exposed to Node through N-API.
//! Planning stays in Rust so TypeScript can display or submit consolidation
//! plans without duplicating strategy-to-operation rules.

use engram_core::plan_consolidation_operations;
use engram_domain::ConsolidationRequest;
use engram_domain::Timestamp;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Deserialize;

use crate::{decode, encode, to_napi_error};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsolidationPlanRequest {
    request: ConsolidationRequest,
    #[serde(default)]
    planned_at: Option<Timestamp>,
}

/// Stateless consolidation behavior exposed to Node through N-API.
///
/// Planning stays in Rust so TypeScript can display or submit consolidation
/// plans without duplicating strategy-to-operation rules.
#[napi]
pub struct NativeConsolidationEngine;

#[napi]
impl NativeConsolidationEngine {
    /// Creates a stateless consolidation planning engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Plans consolidation operations for `{ request, plannedAt? }`.
    #[napi(js_name = "planJson")]
    pub fn plan_json(&self, request_json: String) -> Result<String> {
        let request = decode::<ConsolidationPlanRequest>(&request_json)?;
        let planned_at = request.planned_at.unwrap_or_else(chrono::Utc::now);
        let operations =
            plan_consolidation_operations(&request.request, planned_at).map_err(to_napi_error)?;
        encode(&operations)
    }
}
