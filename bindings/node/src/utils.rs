//! Serialization utilities for Node-API bridge.
//!
//! Helper functions for encoding/decoding JSON and extracting common fields
//! from request values.

use engram_domain::{Id, Scope};
use napi::bindgen_prelude::*;
use serde::{Deserialize, Serialize};

/// Decodes a JSON string into a typed value.
pub fn decode<T>(json: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(json).map_err(|error| Error::from_reason(error.to_string()))
}

/// Encodes a value as a JSON string.
pub fn encode<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    serde_json::to_string(value).map_err(|error| Error::from_reason(error.to_string()))
}

/// Extracts an `Id` field from a JSON value.
pub fn id_field(value: &serde_json::Value, key: &str) -> Result<Id> {
    let text = value
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::from_reason(format!("missing string field '{key}'")))?;
    Ok(Id::from(text))
}

/// Extracts a `Scope` field from a JSON value.
pub fn scope_field(value: &serde_json::Value) -> Result<Scope> {
    let scope_value = value
        .get("scope")
        .ok_or_else(|| Error::from_reason("missing 'scope' field"))?;
    serde_json::from_value::<Scope>(scope_value.clone())
        .map_err(|error| Error::from_reason(format!("invalid scope: {error}")))
}

/// Converts a `CoreError` to an N-API error.
pub fn to_napi_error(error: engram_memory::CoreError) -> Error {
    Error::from_reason(error.to_string())
}
