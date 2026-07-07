//! OpenAPI domain types for contract detection and parsing.
//!
//! Minimal OpenAPI model containing only the fields we inspect for entity
//! and edge extraction. Uses serde(default) for optional fields to gracefully
//! handle missing or null values.

use serde::Deserialize;

/// OpenAPI/Swagger document structure.
#[derive(Debug, Deserialize)]
pub struct OpenApiDoc {
    /// OpenAPI 3.x version marker (e.g. "3.0.0").
    pub openapi: Option<String>,
    /// Swagger 2.x version marker (e.g. "2.0").
    pub swagger: Option<String>,
    /// Path template → path item map.
    #[serde(default)]
    pub paths: std::collections::HashMap<String, PathItem>,
}

/// Path item with HTTP method operations.
#[derive(Debug, Deserialize, Default)]
pub struct PathItem {
    #[serde(default)]
    pub get: Option<Operation>,
    #[serde(default)]
    pub post: Option<Operation>,
    #[serde(default)]
    pub put: Option<Operation>,
    #[serde(default)]
    pub delete: Option<Operation>,
    #[serde(default)]
    pub patch: Option<Operation>,
    #[serde(default)]
    pub head: Option<Operation>,
    #[serde(default)]
    pub options: Option<Operation>,
    #[serde(default)]
    pub trace: Option<Operation>,
}

/// Operation definition with summary and media types.
#[derive(Debug, Deserialize, Default)]
pub struct Operation {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(rename = "requestBody", default)]
    pub request_body: Option<RequestBody>,
    #[serde(default)]
    pub responses: std::collections::HashMap<String, ResponseEntry>,
}

/// Request body content types.
#[derive(Debug, Deserialize)]
pub struct RequestBody {
    #[serde(default)]
    pub content: std::collections::HashMap<String, serde_json::Value>,
}

/// Response entry with content types.
#[derive(Debug, Deserialize)]
pub struct ResponseEntry {
    #[serde(default)]
    pub content: Option<std::collections::HashMap<String, serde_json::Value>>,
}
