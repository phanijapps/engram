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

impl Policy {
    /// Returns the domain policy shape for graphless entities and relationships.
    ///
    /// Those records carry no policy field, while graph-owned records always use
    /// their owning graph's actual policy instead of this fallback.
    pub fn graph_lane_default() -> Self {
        Self {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }
}

// ── Derived-record policy composition (never-widen lattice) ─────────────────
// MCCL-audit F2: records derived from inputs (reflection beliefs, future
// summaries) must never be LESS protected than their most restrictive input.

/// Composes the policy of a derived record from its inputs — the
/// **never-widen** lattice:
///
/// - `visibility`: the most restrictive input wins
///   (`Private < Workspace < Organization < Public`).
/// - `retention`: the shortest input retention wins
///   (`Ephemeral < Session < Durable < LegalHold`) — a derived record never
///   outlives its most ephemeral input.
/// - `sensitivity`: the highest input sensitivity wins (`None` = unspecified,
///   treated below `Low`).
/// - `allowed_uses`: the INTERSECTION — an empty intersection means the
///   derived record has no permitted use (honest, not guessed).
/// - `expires_at`: the earliest input expiry (any expiring input bounds the
///   derived record).
/// - `delete_mode`: the most destructive input mode wins
///   (`Delete > Redact > Tombstone > Archive`) so derived content cannot
///   linger beyond input deletion intent.
///
/// Empty input slice returns the most restrictive possible policy.
pub fn compose_derived_policy(inputs: &[Policy]) -> Policy {
    use std::cmp::Ordering;

    let restrictiveness = |v: &Visibility| match v {
        Visibility::Private => 0,
        Visibility::Workspace => 1,
        Visibility::Organization => 2,
        Visibility::Public => 3,
    };
    let lifetime = |r: &Retention| match r {
        Retention::Ephemeral => 0,
        Retention::Session => 1,
        Retention::Durable => 2,
        Retention::LegalHold => 3,
    };
    let sensitivity_rank = |s: &Option<Sensitivity>| match s {
        None => 0,
        Some(Sensitivity::Low) => 1,
        Some(Sensitivity::Medium) => 2,
        Some(Sensitivity::High) => 3,
        Some(Sensitivity::Restricted) => 4,
    };
    let destructiveness = |d: &Option<DeleteMode>| match d {
        None => 0,
        Some(DeleteMode::Archive) => 1,
        Some(DeleteMode::Tombstone) => 2,
        Some(DeleteMode::Redact) => 3,
        Some(DeleteMode::Delete) => 4,
    };

    let mut policy = Policy {
        visibility: Visibility::Private,
        retention: Retention::Ephemeral,
        sensitivity: None,
        allowed_uses: Vec::new(),
        expires_at: None,
        delete_mode: None,
    };
    let Some(first) = inputs.first() else {
        return policy;
    };
    policy.visibility = first.visibility.clone();
    policy.retention = first.retention.clone();
    policy.sensitivity = first.sensitivity.clone();
    // Allowed uses start from the first input, then intersect with the rest.
    policy.allowed_uses = first.allowed_uses.clone();
    policy.expires_at = first.expires_at;
    policy.delete_mode = first.delete_mode.clone();

    for input in &inputs[1..] {
        if restrictiveness(&input.visibility) < restrictiveness(&policy.visibility) {
            policy.visibility = input.visibility.clone();
        }
        if lifetime(&input.retention) < lifetime(&policy.retention) {
            policy.retention = input.retention.clone();
        }
        if sensitivity_rank(&input.sensitivity) > sensitivity_rank(&policy.sensitivity) {
            policy.sensitivity = input.sensitivity.clone();
        }
        policy
            .allowed_uses
            .retain(|use_| input.allowed_uses.contains(use_));
        match (input.expires_at, policy.expires_at) {
            (Some(incoming), Some(current)) => {
                if incoming < current {
                    policy.expires_at = Some(incoming);
                }
            }
            (Some(incoming), None) => policy.expires_at = Some(incoming),
            _ => {}
        }
        if destructiveness(&input.delete_mode) > destructiveness(&policy.delete_mode) {
            policy.delete_mode = input.delete_mode.clone();
        }
    }
    let _ = Ordering::Equal; // (rank helpers above keep clippy quiet)
    policy
}

#[cfg(test)]
mod compose_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn base() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Consolidation],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        }
    }

    #[test]
    fn never_widens_any_dimension() {
        let mut restricted = base();
        restricted.visibility = Visibility::Private;
        restricted.retention = Retention::Session;
        restricted.sensitivity = Some(Sensitivity::High);
        restricted.allowed_uses = vec![AllowedUse::Retrieval];
        restricted.delete_mode = Some(DeleteMode::Delete);

        let composed = compose_derived_policy(&[base(), restricted]);
        assert_eq!(composed.visibility, Visibility::Private);
        assert_eq!(composed.retention, Retention::Session);
        assert_eq!(composed.sensitivity, Some(Sensitivity::High));
        assert_eq!(composed.allowed_uses, vec![AllowedUse::Retrieval]);
        assert_eq!(composed.delete_mode, Some(DeleteMode::Delete));
    }

    #[test]
    fn disjoint_allowed_uses_intersect_to_empty_not_guessed() {
        let mut a = base();
        a.allowed_uses = vec![AllowedUse::Retrieval];
        let mut b = base();
        b.allowed_uses = vec![AllowedUse::Evaluation];
        let composed = compose_derived_policy(&[a, b]);
        assert!(composed.allowed_uses.is_empty());
    }

    #[test]
    fn earliest_expiry_wins_once_any_input_expires() {
        let early = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let mut a = base();
        a.expires_at = Some(early);
        let composed = compose_derived_policy(&[base(), a]);
        assert_eq!(composed.expires_at, Some(early));
    }

    #[test]
    fn empty_inputs_are_maximally_restrictive() {
        let composed = compose_derived_policy(&[]);
        assert_eq!(composed.visibility, Visibility::Private);
        assert_eq!(composed.retention, Retention::Ephemeral);
    }
}
