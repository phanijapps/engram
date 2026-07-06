//! Ontology operations for the knowledge engine.
//!
//! Manages ontology, class, property, and axiom operations through the
//! OntologyRepository port.

use engram_domain::{Id, Ontology, OntologyAxiom, OntologyClass, OntologyProperty, Scope};
use engram_knowledge::OntologyRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

/// Stores or updates an ontology.
pub fn put_ontology_json(store: &Arc<SqlKnowledgeStore>, ontology_json: String) -> Result<String> {
    let ontology: Ontology = decode(&ontology_json)?;
    let result = block_on(store.put_ontology(ontology)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves an ontology by ID and scope.
pub fn get_ontology_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_ontology(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Stores or updates an ontology class.
pub fn put_class_json(store: &Arc<SqlKnowledgeStore>, class_json: String) -> Result<String> {
    let class: OntologyClass = decode(&class_json)?;
    let result = block_on(store.put_class(class)).map_err(to_napi_error)?;
    encode(&result)
}

/// Stores or updates an ontology property.
pub fn put_property_json(store: &Arc<SqlKnowledgeStore>, property_json: String) -> Result<String> {
    let property: OntologyProperty = decode(&property_json)?;
    let result = block_on(store.put_property(property)).map_err(to_napi_error)?;
    encode(&result)
}

/// Stores or updates an ontology axiom.
pub fn put_axiom_json(store: &Arc<SqlKnowledgeStore>, axiom_json: String) -> Result<String> {
    let axiom: OntologyAxiom = decode(&axiom_json)?;
    let result = block_on(store.put_axiom(axiom)).map_err(to_napi_error)?;
    encode(&result)
}
