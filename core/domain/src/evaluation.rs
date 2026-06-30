//! Portable evaluation fixtures for memory quality checks.
//!
//! Fixtures describe seed data, retrieval requests, expected inclusions, and
//! forbidden results. They are intentionally provider-neutral so the same cases
//! can run against in-memory tests, SQL/vector adapters, and future bindings.

use serde::{Deserialize, Serialize};

use crate::{
    EvaluationId, KnowledgeChunk, KnowledgeSource, RetrievalRequest, RetrievalTargetType, Scope,
    SourceDocument, Timestamp, WriteMemoryRequest,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationFixture {
    pub id: EvaluationId,
    pub name: String,
    pub scope: Scope,
    pub setup: EvaluationSetup,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cases: Vec<EvaluationCase>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationSetup {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub memories: Vec<WriteMemoryRequest>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sources: Vec<KnowledgeSource>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub documents: Vec<SourceDocument>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub chunks: Vec<KnowledgeChunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationCase {
    pub id: String,
    pub request: RetrievalRequest,
    pub expect: EvaluationExpectation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationExpectation {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub must_include: Vec<ExpectedTarget>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub must_exclude: Vec<ExpectedTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_explanation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedTarget {
    pub target_type: RetrievalTargetType,
    pub target_id: String,
}
