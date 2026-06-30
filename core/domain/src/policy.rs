//! Policy models that gate retrieval, retention, and deletion behavior.
//!
//! Policies are typed contract data, not advisory metadata. Core services and
//! adapters must enforce visibility, retention, sensitivity, allowed uses, and
//! delete mode before returning or mutating records.

use serde::{Deserialize, Serialize};

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Workspace,
    Organization,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Ephemeral,
    Session,
    Durable,
    LegalHold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Low,
    Medium,
    High,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedUse {
    Retrieval,
    Personalization,
    Evaluation,
    Consolidation,
    TrainingExport,
    Debugging,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    Delete,
    Redact,
    Tombstone,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub visibility: Visibility,
    pub retention: Retention,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<Sensitivity>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub allowed_uses: Vec<AllowedUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_mode: Option<DeleteMode>,
}
