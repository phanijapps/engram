//! Architecture evaluation engine for Node-API bridge.
//!
//! Stateless architecture evaluation behavior exposed to Node through N-API.
//! Evaluates architecture coverage against required capabilities.

use engram_core::{
    ArchitectureEvalCase, required_architecture_capabilities, summarize_architecture_coverage,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{decode, encode};

/// Stateless architecture evaluation behavior exposed to Node through N-API.
#[napi]
pub struct NativeEvalEngine;

#[napi]
impl NativeEvalEngine {
    /// Creates a stateless architecture evaluation engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Evaluates architecture coverage for a JSON array of `ArchitectureEvalCase`.
    #[napi(js_name = "architectureCoverageJson")]
    pub fn architecture_coverage_json(&self, cases_json: String) -> Result<String> {
        let cases = decode::<Vec<ArchitectureEvalCase>>(&cases_json)?;
        let required = required_architecture_capabilities();
        let coverage = summarize_architecture_coverage(cases, &required);
        encode(&coverage)
    }
}
