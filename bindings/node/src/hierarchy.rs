//! Hierarchy validation engine for Node-API bridge.
//!
//! Stateless hierarchy behavior exposed to Node through N-API.
//! This transport validates hierarchy build outputs with Rust-owned rules. It
//! does not persist nodes or replace the SQLite hierarchy adapter.

use engram_core::validate_hierarchy_parentage;
use engram_domain::HierarchyNode;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{decode, encode, to_napi_error};

/// Stateless hierarchy behavior exposed to Node through N-API.
///
/// This transport validates hierarchy build outputs with Rust-owned rules. It
/// does not persist nodes or replace the SQLite hierarchy adapter.
#[napi]
pub struct NativeHierarchyEngine;

#[napi]
impl NativeHierarchyEngine {
    /// Creates a stateless hierarchy validation engine.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self
    }

    /// Validates hierarchy parentage for a JSON array of `HierarchyNode`.
    #[napi(js_name = "validateParentageJson")]
    pub fn validate_parentage_json(&self, nodes_json: String) -> Result<String> {
        let nodes = decode::<Vec<HierarchyNode>>(&nodes_json)?;
        validate_hierarchy_parentage(&nodes).map_err(to_napi_error)?;
        encode(&serde_json::json!({ "valid": true }))
    }
}

impl Default for NativeHierarchyEngine {
    fn default() -> Self {
        Self::new()
    }
}
