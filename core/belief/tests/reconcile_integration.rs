//! End-to-end reconciliation over a mixed fixture: tier precedence, bitemporal
//! exclusion, and one equal-authority conflict, driven through the public
//! `reconcile` surface.

use chrono::{TimeZone, Utc};
use engram_belief::{AuthorityPolicy, reconcile};
use engram_domain::{
    Actor, ActorKind, AssertionReviewStatus, AuthorityTier, BeliefSourceTargetType, BeliefSubject,
    ContradictionTargetType, Id, Policy, Provenance, Retention, Scalar, Scope, SourceAssertion,
    Timestamp, Visibility,
};
use serde_json::json;

fn ts(seconds: i64) -> Timestamp {
    Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
}

fn assertion(
    id: &str,
    subject_key: &str,
    predicate: &str,
    object: Scalar,
    tier: AuthorityTier,
    valid: (Option<i64>, Option<i64>),
) -> SourceAssertion {
    SourceAssertion {
        id: Id::from(id),
        scope: Scope {
            tenant: "tenant-a".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
        subject: BeliefSubject {
            key: subject_key.to_owned(),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        predicate: predicate.to_owned(),
        object,
        source_system: "catalog".to_owned(),
        source_record_id: id.to_owned(),
        source_uri: None,
        authority_level: tier,
        confidence: Some(0.8),
        valid_from: valid.0.map(ts),
        valid_until: valid.1.map(ts),
        asserted_at: ts(1),
        review_status: AssertionReviewStatus::Reviewed,
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: Vec::new(),
            expires_at: None,
            delete_mode: None,
        },
        provenance: Provenance {
            source: "catalog".to_owned(),
            actor: Actor {
                id: Id::from("sys"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: ts(1),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(0.8),
            method: None,
        },
    }
}

#[test]
fn reconciles_mixed_fixture_into_beliefs_and_one_contradiction() {
    let fixture = vec![
        // (1) owned_by: Primary beats a live Secondary -> belief team-blue.
        assertion(
            "own-primary",
            "system:auth",
            "owned_by",
            json!("team-blue"),
            AuthorityTier::Primary,
            (Some(0), None),
        ),
        assertion(
            "own-secondary",
            "system:auth",
            "owned_by",
            json!("team-red"),
            AuthorityTier::Secondary,
            (Some(0), None),
        ),
        // (2) tier: an expired Primary [0,20) is excluded at t=100; the live
        //     Secondary wins -> belief "gold".
        assertion(
            "tier-primary-expired",
            "system:auth",
            "tier",
            json!("platinum"),
            AuthorityTier::Primary,
            (Some(0), Some(20)),
        ),
        assertion(
            "tier-secondary-live",
            "system:auth",
            "tier",
            json!("gold"),
            AuthorityTier::Secondary,
            (Some(0), None),
        ),
        // (3) region: two live equal-authority Primaries disagree -> contradiction.
        assertion(
            "region-a",
            "system:auth",
            "region",
            json!("us-east"),
            AuthorityTier::Primary,
            (Some(0), None),
        ),
        assertion(
            "region-b",
            "system:auth",
            "region",
            json!("us-west"),
            AuthorityTier::Primary,
            (Some(0), None),
        ),
    ];

    let out =
        reconcile(&fixture, ts(100), &AuthorityPolicy::personal_default()).expect("reconcile");

    // Two beliefs (owned_by, tier); region produced a contradiction instead.
    assert_eq!(out.beliefs.len(), 2, "beliefs: {:?}", out.beliefs);
    assert_eq!(out.contradictions.len(), 1);

    let owned = out
        .beliefs
        .iter()
        .find(|b| b.content.starts_with("owned_by"))
        .expect("owned_by belief");
    assert!(owned.content.contains("team-blue"));
    assert_eq!(
        owned.sources[0].target_type,
        BeliefSourceTargetType::Assertion
    );
    assert_eq!(owned.sources[0].target_id, "own-primary");

    let tier = out
        .beliefs
        .iter()
        .find(|b| b.content.starts_with("tier"))
        .expect("tier belief");
    assert!(tier.content.contains("gold"));
    assert_eq!(tier.sources[0].target_id, "tier-secondary-live");

    let contradiction = &out.contradictions[0];
    assert_eq!(contradiction.targets.len(), 2);
    assert!(
        contradiction
            .targets
            .iter()
            .all(|t| t.target_type == ContradictionTargetType::Assertion)
    );
}
