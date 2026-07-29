//! The handler context every tool shares.
//!
//! One [`App`] is constructed at startup and borrowed by every tool handler:
//! the [`EngramProvider`] every capability routes through, the fused-per-project
//! [`Scope`], and the active ontology/taxonomy configuration. Keeping these on a
//! single context (rather than threading them per tool) is what lets the generic
//! `<C>` registry stay simple.

use engram_domain::Scope;
use engram_integration::EngramProvider;
use serde_json::Value;

use crate::ontology::{
    OntologyConfig, TaxonomyConfig, ontology_config_as_text, taxonomy_config_as_text,
};
use crate::registry::ToolError;

/// Shared, immutable handler context.
#[allow(dead_code)] // provider + scope are read by the write/recall tools (T5+)
pub struct App {
    pub provider: EngramProvider,
    pub scope: Scope,
    pub ontology: OntologyConfig,
    pub taxonomy: TaxonomyConfig,
}

/// `ontology_read`: return the active multi-layer ontology config.
pub fn ontology_read(app: &App, _args: &Value) -> Result<Value, ToolError> {
    Ok(ontology_config_as_text(&app.ontology))
}

/// `taxonomy_read`: return the active taxonomy config.
pub fn taxonomy_read(app: &App, _args: &Value) -> Result<Value, ToolError> {
    Ok(taxonomy_config_as_text(&app.taxonomy))
}
