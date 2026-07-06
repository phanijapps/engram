//! Concept and taxonomy operations for the knowledge engine.
//!
//! Manages concept schemes, concepts, and taxonomy validation through the
//! TaxonomyRepository port.

use engram_domain::{Concept, ConceptRelation, ConceptScheme, Id, Scope, TaxonomyProposal};
use engram_knowledge::{TaxonomyRepository, validate_taxonomy_proposal};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaxonomyValidationRequest {
    proposal: TaxonomyProposal,
    concepts: Vec<Concept>,
    #[serde(default)]
    relations: Vec<ConceptRelation>,
}

/// Stores or updates a concept scheme.
pub fn put_concept_scheme_json(
    store: &Arc<SqlKnowledgeStore>,
    scheme_json: String,
) -> Result<String> {
    let scheme: ConceptScheme = decode(&scheme_json)?;
    let result = block_on(store.put_concept_scheme(scheme)).map_err(to_napi_error)?;
    encode(&result)
}

/// Retrieves a concept scheme by ID and scope.
pub fn get_concept_scheme_json(
    store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let id = id_field(&value, "id")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.get_concept_scheme(&id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Stores or updates a concept.
pub fn put_concept_json(store: &Arc<SqlKnowledgeStore>, concept_json: String) -> Result<String> {
    let concept: Concept = decode(&concept_json)?;
    let result = block_on(store.put_concept(concept)).map_err(to_napi_error)?;
    encode(&result)
}

/// Stores or updates a concept relation.
pub fn put_concept_relation_json(
    store: &Arc<SqlKnowledgeStore>,
    relation_json: String,
) -> Result<String> {
    let relation: ConceptRelation = decode(&relation_json)?;
    let result = block_on(store.put_concept_relation(relation)).map_err(to_napi_error)?;
    encode(&result)
}

/// Lists concepts in a scheme.
pub fn list_concepts_json(store: &Arc<SqlKnowledgeStore>, request_json: String) -> Result<String> {
    let value = decode::<serde_json::Value>(&request_json)?;
    let scheme_id = id_field(&value, "schemeId")?;
    let scope = scope_field(&value)?;
    let result = block_on(store.list_concepts(&scheme_id, &scope)).map_err(to_napi_error)?;
    encode(&result)
}

/// Validates a governed taxonomy proposal using Rust-owned taxonomy rules.
///
/// The input is `{ proposal, concepts, relations }`; the result is a
/// `TaxonomyValidationReport` JSON payload. No store mutation occurs.
pub fn validate_taxonomy_proposal_json(
    _store: &Arc<SqlKnowledgeStore>,
    request_json: String,
) -> Result<String> {
    let request = decode::<TaxonomyValidationRequest>(&request_json)?;
    let report =
        validate_taxonomy_proposal(&request.proposal, &request.concepts, &request.relations);
    encode(&report)
}
