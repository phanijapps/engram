//! Attributed recall outcomes (F2: the outcome loop).
//!
//! The host reports what happened AFTER a recall-informed action: did the
//! task succeed? Every record is ATTRIBUTED (query id, recalled/omitted
//! targets, config version, verifier) and NON-CAUSAL by contract — outcome
//! data describes ASSOCIATION between recalls and results; causal credit
//! requires matched interventions (the sequential harness), and surfaces
//! must never claim otherwise.

use serde::{Deserialize, Serialize};

use crate::{Actor, Id, Scope, Timestamp};

/// The host-reported result of one recall-informed action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallOutcome {
    /// The FusionTrace query id of the recall this outcome attributes.
    pub query_id: String,
    pub scope: Scope,
    /// Targets that appeared in the composed context (ids).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub recalled_targets: Vec<String>,
    /// Targets the budget/policy omitted (ids) — recorded so POTENTIAL
    /// omission-related losses can be investigated; whether inclusion
    /// would have helped remains unproven without matched interventions.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub omitted_targets: Vec<String>,
    /// Version of the fusion/trust configuration in effect (from
    /// CapabilityReport/observability), so ablations are attributable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_version: Option<String>,
    pub outcome: TaskOutcome,
    pub occurred_at: Timestamp,
}

/// The task result + who verified it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOutcome {
    pub success: bool,
    /// Host-defined capability label (e.g. "payment-timeout-runbook").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// The actor/system that verified the result (hosts verify; engram
    /// never does — it is not a task executor).
    pub verifier: Actor,
}

/// Append-only outcome record with storage identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedOutcome {
    pub id: Id,
    #[serde(flatten)]
    pub outcome: RecallOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportOutcomeRequest {
    #[serde(flatten)]
    pub outcome: RecallOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportOutcomeResult {
    pub recorded: RecordedOutcome,
}

/// Time-windowed outcome summary (the ΔP reporter's data source).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeWindow {
    pub scope: Scope,
    pub since: Timestamp,
    pub until: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_serializes_camel_case_with_attribution() {
        let record = RecallOutcome {
            query_id: "q-1".to_owned(),
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            recalled_targets: vec!["memory-1".into()],
            omitted_targets: vec![],
            config_version: Some("fusion-v2".into()),
            outcome: TaskOutcome {
                success: true,
                capability: Some("deploy-gate".into()),
                verifier: Actor {
                    id: crate::ActorId::from("host-checker"),
                    kind: crate::ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
            },
            occurred_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(json.contains("\"queryId\""), "{json}");
        assert!(json.contains("\"recalledTargets\""), "{json}");
        let back: RecallOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, record);
    }
}
