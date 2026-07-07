//! Authority-weighted survivorship reconciliation of source assertions.
//!
//! [`reconcile`] takes competing [`SourceAssertion`]s and an [`AuthorityPolicy`]
//! and derives, per `(subject, predicate)`, the belief held by the highest-
//! authority source that is live at a point in time. Authority — not recency —
//! decides the winner. When equal-top-authority sources disagree over an
//! overlapping valid interval, no belief is derived and an advisory
//! [`Contradiction`] is emitted instead: the layer exposes tension, it never
//! silently overwrites.
//!
//! The function is pure and deterministic: all constructed records take their
//! timestamps from the `at` argument, so there is no clock or id-generator
//! dependency. The authority policy is a value passed in, so named profiles are
//! future configuration data, not a code change.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use engram_domain::{
    Actor, ActorKind, AuthorityTier, Belief, BeliefSource, BeliefSourceTargetType, BeliefStatus,
    Contradiction, ContradictionKind, ContradictionStatus, ContradictionTarget,
    ContradictionTargetType, DerivationKind, DerivationRef, Id, Provenance, Scalar, Scope,
    SourceAssertion, Timestamp,
};
use engram_runtime::{CoreError, CoreResult};

use crate::temporal::live_at;

/// How an equal-top-authority disagreement is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TieRule {
    /// Emit a `Contradiction` and derive no belief. The only rule this slice
    /// ships; confidence never adjudicates a same-tier disagreement.
    ContradictionOnTie,
}

/// The authority ordering and tie rule reconciliation runs under.
///
/// `tiers_high_to_low` is the ranking: earlier is more authoritative. It is a
/// plain value, so a profile (personal, enterprise, autonomous) is a preset of
/// this struct rather than a compiled-in branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityPolicy {
    pub tiers_high_to_low: Vec<AuthorityTier>,
    pub tie: TieRule,
}

impl AuthorityPolicy {
    /// The default policy for a personal/coding-agent profile: a source's
    /// declared tier is honored high-to-low, ties raise a contradiction.
    pub fn personal_default() -> Self {
        Self {
            tiers_high_to_low: vec![
                AuthorityTier::Primary,
                AuthorityTier::Secondary,
                AuthorityTier::Inferred,
            ],
            tie: TieRule::ContradictionOnTie,
        }
    }

    /// Rank of a tier: lower is more authoritative. A tier absent from the
    /// policy ranks below every listed tier.
    fn rank(&self, tier: AuthorityTier) -> usize {
        self.tiers_high_to_low
            .iter()
            .position(|t| *t == tier)
            .unwrap_or(self.tiers_high_to_low.len())
    }

    fn validate(&self) -> CoreResult<()> {
        if self.tiers_high_to_low.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "authority policy has no tiers to rank by".to_owned(),
            });
        }
        let mut seen: Vec<AuthorityTier> = Vec::new();
        for tier in &self.tiers_high_to_low {
            if seen.contains(tier) {
                return Err(CoreError::InvalidRequest {
                    reason: "authority policy lists a tier more than once".to_owned(),
                });
            }
            seen.push(*tier);
        }
        Ok(())
    }
}

/// The records produced by one reconciliation pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconciled {
    pub beliefs: Vec<Belief>,
    pub contradictions: Vec<Contradiction>,
}

/// Reconciles competing source assertions into beliefs and advisory
/// contradictions, evaluated at `at` under `policy`.
///
/// Returns `InvalidRequest` when the policy has no tiers or lists one twice.
pub fn reconcile(
    assertions: &[SourceAssertion],
    at: Timestamp,
    policy: &AuthorityPolicy,
) -> CoreResult<Reconciled> {
    policy.validate()?;

    let mut groups: BTreeMap<(String, String), Vec<&SourceAssertion>> = BTreeMap::new();
    for assertion in assertions {
        groups
            .entry((assertion.subject.key.clone(), assertion.predicate.clone()))
            .or_default()
            .push(assertion);
    }

    let mut beliefs = Vec::new();
    let mut contradictions = Vec::new();

    for (_group_key, members) in groups {
        // Only assertions live at `at` compete. A disjoint-validity pair never
        // co-occurs here, so sequential truth is not a contradiction. A
        // malformed interval (`valid_until <= valid_from`) is never live.
        let live: Vec<&SourceAssertion> = members
            .into_iter()
            .filter(|a| live_at(a.valid_from, a.valid_until, at))
            .collect();
        if live.is_empty() {
            continue;
        }
        let live_count = live.len();

        let top_rank = live
            .iter()
            .map(|a| policy.rank(a.authority_level))
            .min()
            .expect("live is non-empty");
        let top: Vec<&SourceAssertion> = live
            .into_iter()
            .filter(|a| policy.rank(a.authority_level) == top_rank)
            .collect();

        // Distinct objects among the top tier decide agree-vs-disagree.
        // Confidence is never consulted here — it only orders agreeing sources.
        // O(n^2) over the top tier, which is assumed small (the competing
        // authoritative sources for one subject/predicate), so no index is kept.
        let mut distinct_objects: Vec<&Scalar> = Vec::new();
        for assertion in &top {
            if !distinct_objects.iter().any(|o| **o == assertion.object) {
                distinct_objects.push(&assertion.object);
            }
        }

        if distinct_objects.len() >= 2 {
            contradictions.push(build_contradiction(&top, distinct_objects.len(), at));
        } else {
            beliefs.push(build_belief(pick_winner(&top), live_count, at));
        }
    }

    Ok(Reconciled {
        beliefs,
        contradictions,
    })
}

/// Among assertions that agree on `object`, pick the one to cite: highest
/// confidence, then most recently asserted, then smallest id — deterministic.
///
/// Absent confidence sorts last here (as `0.0`) so a source that states its
/// confidence is preferred as the citation. This is deliberately the opposite
/// of `build_belief`/`build_contradiction`, where absent confidence is read as
/// full (`1.0`): those set a *magnitude* (belief confidence, contradiction
/// severity) where "unstated" conservatively means "not discounted", whereas
/// here we are *ranking* and want a stated value to win the tie.
fn pick_winner<'a>(top: &[&'a SourceAssertion]) -> &'a SourceAssertion {
    let mut ordered = top.to_vec();
    ordered.sort_by(|a, b| {
        let ca = a.confidence.unwrap_or(0.0);
        let cb = b.confidence.unwrap_or(0.0);
        cb.partial_cmp(&ca)
            .unwrap_or(Ordering::Equal)
            .then(b.asserted_at.cmp(&a.asserted_at))
            .then(a.id.to_string().cmp(&b.id.to_string()))
    });
    ordered[0]
}

fn build_belief(winner: &SourceAssertion, live_count: usize, at: Timestamp) -> Belief {
    let derivation = DerivationRef {
        kind: DerivationKind::Consolidation,
        model: None,
        prompt_hash: None,
        input_refs: Vec::new(),
        created_at: at,
    };
    Belief {
        id: Id::from(record_id(
            "belief",
            &[
                &scope_key(&winner.scope),
                &winner.subject.key,
                &winner.predicate,
            ],
        )),
        scope: winner.scope.clone(),
        subject: winner.subject.clone(),
        content: format!("{} {}", winner.predicate, winner.object),
        status: BeliefStatus::Active,
        confidence: winner.confidence.unwrap_or(1.0),
        sources: vec![BeliefSource {
            target_type: BeliefSourceTargetType::Assertion,
            target_id: winner.id.to_string(),
            authority_level: Some(winner.authority_level),
            weight: None,
            confidence: winner.confidence,
            valid_from: winner.valid_from,
            valid_until: winner.valid_until,
        }],
        valid_from: winner.valid_from,
        valid_until: winner.valid_until,
        superseded_by: None,
        stale: None,
        synthesizer: Some(derivation.clone()),
        reasoning: Some(format!(
            "selected {:?}-authority assertion `{}` among {} live competing assertion(s)",
            winner.authority_level, winner.id, live_count
        )),
        embedding_refs: Vec::new(),
        policy: winner.policy.clone(),
        provenance: Provenance {
            source: winner.provenance.source.clone(),
            actor: winner.provenance.actor.clone(),
            observed_at: at,
            evidence: Vec::new(),
            derivations: vec![derivation],
            confidence: winner.confidence,
            method: Some("authority_survivorship".to_owned()),
        },
        created_at: at,
        updated_at: None,
        metadata: None,
    }
}

fn build_contradiction(
    top: &[&SourceAssertion],
    distinct_claims: usize,
    at: Timestamp,
) -> Contradiction {
    // Severity mirrors the existing ContradictionDetector: the max confidence
    // over the conflicting records, clamped to the normalized range. Absent
    // confidence defaults to 1.0 so the fold is total.
    let severity = top
        .iter()
        .map(|a| a.confidence.unwrap_or(1.0))
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0);
    let targets = top
        .iter()
        .map(|a| ContradictionTarget {
            target_type: ContradictionTargetType::Assertion,
            target_id: a.id.to_string(),
            role: None,
        })
        .collect();
    let anchor = top[0];
    Contradiction {
        id: Id::from(record_id(
            "contradiction",
            &[
                &scope_key(&anchor.scope),
                &anchor.subject.key,
                &anchor.predicate,
            ],
        )),
        scope: anchor.scope.clone(),
        kind: ContradictionKind::Logical,
        targets,
        severity,
        status: ContradictionStatus::Open,
        reasoning: Some(format!(
            "{} top-authority assertions on `{}`/`{}` carry {} distinct claims",
            top.len(),
            anchor.subject.key,
            anchor.predicate,
            distinct_claims
        )),
        detected_by: Some(DerivationRef {
            kind: DerivationKind::Consolidation,
            model: None,
            prompt_hash: None,
            input_refs: Vec::new(),
            created_at: at,
        }),
        resolution: None,
        provenance: reconciler_provenance(at),
        detected_at: at,
        updated_at: None,
    }
}

/// Length-prefixed join of id components, so no component's contents can be
/// confused with a separator. Subject keys and scope fields may themselves
/// contain `:` (e.g. `system:auth`), so a plain `:`-join would let
/// `(subject="a:b", predicate="c")` and `(subject="a", predicate="b:c")`
/// collide; prefixing each part with its byte length removes the ambiguity.
fn record_id(prefix: &str, parts: &[&str]) -> String {
    let mut out = String::from(prefix);
    for part in parts {
        out.push_str(&format!(":{}:{}", part.len(), part));
    }
    out
}

/// Full scope discriminator for derived-record ids, so two single-scope passes
/// that share a tenant but differ in workspace/session/subject/environment do
/// not collide on the same `(subject, predicate)`. Length-prefixed for the same
/// reason as `record_id`.
fn scope_key(scope: &Scope) -> String {
    record_id(
        "scope",
        &[
            &scope.tenant,
            scope.workspace.as_deref().unwrap_or(""),
            scope.session.as_deref().unwrap_or(""),
            scope.subject.as_deref().unwrap_or(""),
            scope.environment.as_deref().unwrap_or(""),
        ],
    )
}

fn reconciler_provenance(at: Timestamp) -> Provenance {
    Provenance {
        source: "belief-reconciler".to_owned(),
        actor: Actor {
            id: Id::from("engram-belief-reconciler"),
            kind: ActorKind::System,
            display_name: Some("Belief reconciler".to_owned()),
            metadata: None,
        },
        observed_at: at,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("authority_survivorship".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use engram_domain::{
        AssertionReviewStatus, BeliefSubject, Policy, Retention, Scope, Visibility,
    };
    use serde_json::json;

    use super::*;

    fn ts(seconds: i64) -> Timestamp {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn scope() -> Scope {
        Scope {
            tenant: "t1".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn policy_value() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: Vec::new(),
            expires_at: None,
            delete_mode: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "catalog".to_owned(),
            actor: Actor {
                id: Id::from("sys"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: ts(0),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: None,
            method: None,
        }
    }

    struct Build {
        id: &'static str,
        predicate: &'static str,
        object: Scalar,
        tier: AuthorityTier,
        confidence: Option<f32>,
        valid_from: Option<i64>,
        valid_until: Option<i64>,
        asserted_at: i64,
    }

    fn assertion(b: Build) -> SourceAssertion {
        SourceAssertion {
            id: Id::from(b.id),
            scope: scope(),
            subject: BeliefSubject {
                key: "system:auth".to_owned(),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            predicate: b.predicate.to_owned(),
            object: b.object,
            source_system: "catalog".to_owned(),
            source_record_id: b.id.to_owned(),
            source_uri: None,
            authority_level: b.tier,
            confidence: b.confidence,
            valid_from: b.valid_from.map(ts),
            valid_until: b.valid_until.map(ts),
            asserted_at: ts(b.asserted_at),
            review_status: AssertionReviewStatus::Reviewed,
            policy: policy_value(),
            provenance: provenance(),
        }
    }

    fn owned_by(id: &'static str, team: &'static str, tier: AuthorityTier, asserted: i64) -> Build {
        Build {
            id,
            predicate: "owned_by",
            object: json!(team),
            tier,
            confidence: Some(0.9),
            valid_from: None,
            valid_until: None,
            asserted_at: asserted,
        }
    }

    // --- T2: authority-weighted survivorship selection ---

    #[test]
    fn higher_authority_beats_more_recent_lower_authority() {
        let primary = assertion(owned_by(
            "a-primary",
            "team-blue",
            AuthorityTier::Primary,
            10,
        ));
        let mut recent_secondary = assertion(owned_by(
            "b-secondary",
            "team-red",
            AuthorityTier::Secondary,
            999,
        ));
        recent_secondary.confidence = Some(1.0); // even higher confidence + newer
        let out = reconcile(
            &[recent_secondary, primary],
            ts(500),
            &AuthorityPolicy::personal_default(),
        )
        .unwrap();
        assert_eq!(out.beliefs.len(), 1);
        assert_eq!(out.contradictions.len(), 0);
        assert!(out.beliefs[0].content.contains("team-blue"));
    }

    #[test]
    fn swapping_policy_order_flips_the_winner_without_code_change() {
        let primary = assertion(owned_by("a", "team-blue", AuthorityTier::Primary, 10));
        let inferred = assertion(owned_by("b", "team-red", AuthorityTier::Inferred, 10));
        let assertions = [primary, inferred];

        let default = reconcile(&assertions, ts(50), &AuthorityPolicy::personal_default()).unwrap();
        assert!(default.beliefs[0].content.contains("team-blue"));

        let inverted = AuthorityPolicy {
            tiers_high_to_low: vec![
                AuthorityTier::Inferred,
                AuthorityTier::Secondary,
                AuthorityTier::Primary,
            ],
            tie: TieRule::ContradictionOnTie,
        };
        let flipped = reconcile(&assertions, ts(50), &inverted).unwrap();
        assert!(flipped.beliefs[0].content.contains("team-red"));
    }

    #[test]
    fn assertion_outside_valid_interval_does_not_win() {
        // Primary is valid only [0,20); at t=50 only the secondary is live.
        let expired_primary = assertion(Build {
            valid_from: Some(0),
            valid_until: Some(20),
            ..owned_by("a-primary", "team-blue", AuthorityTier::Primary, 5)
        });
        let live_secondary = assertion(Build {
            valid_from: Some(0),
            valid_until: None,
            ..owned_by("b-secondary", "team-red", AuthorityTier::Secondary, 5)
        });
        let out = reconcile(
            &[expired_primary, live_secondary],
            ts(50),
            &AuthorityPolicy::personal_default(),
        )
        .unwrap();
        assert_eq!(out.beliefs.len(), 1);
        assert!(out.beliefs[0].content.contains("team-red"));
    }

    #[test]
    fn malformed_interval_is_never_live() {
        let broken = assertion(Build {
            valid_from: Some(20),
            valid_until: Some(10),
            ..owned_by("a", "team-blue", AuthorityTier::Primary, 5)
        });
        let out = reconcile(&[broken], ts(15), &AuthorityPolicy::personal_default()).unwrap();
        assert!(out.beliefs.is_empty());
        assert!(out.contradictions.is_empty());
    }

    #[test]
    fn empty_policy_is_invalid_request() {
        let err = reconcile(
            &[],
            ts(0),
            &AuthorityPolicy {
                tiers_high_to_low: Vec::new(),
                tie: TieRule::ContradictionOnTie,
            },
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::InvalidRequest { .. }));
    }

    #[test]
    fn duplicated_policy_tier_is_invalid_request() {
        let err = reconcile(
            &[],
            ts(0),
            &AuthorityPolicy {
                tiers_high_to_low: vec![AuthorityTier::Primary, AuthorityTier::Primary],
                tie: TieRule::ContradictionOnTie,
            },
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::InvalidRequest { .. }));
    }

    // --- T3: belief derivation + provenance ---

    #[test]
    fn derived_belief_cites_winning_assertion_and_inherits_scope() {
        let mut winner = assertion(owned_by(
            "a-primary",
            "team-blue",
            AuthorityTier::Primary,
            10,
        ));
        winner.scope.workspace = Some("ws-1".to_owned());
        winner.valid_from = Some(ts(0));
        let out = reconcile(&[winner], ts(50), &AuthorityPolicy::personal_default()).unwrap();
        let belief = &out.beliefs[0];
        assert_eq!(belief.sources.len(), 1);
        assert_eq!(
            belief.sources[0].target_type,
            BeliefSourceTargetType::Assertion
        );
        assert_eq!(belief.sources[0].target_id, "a-primary");
        assert_eq!(
            belief.sources[0].authority_level,
            Some(AuthorityTier::Primary)
        );
        assert_eq!(belief.scope.workspace.as_deref(), Some("ws-1"));
        assert_eq!(belief.valid_from, Some(ts(0)));
        assert!(belief.content.contains("owned_by"));
        assert!(belief.content.contains("team-blue"));
        assert_eq!(
            belief.provenance.derivations[0].kind,
            DerivationKind::Consolidation
        );
    }

    #[test]
    fn absent_confidence_defaults_to_one() {
        let winner = assertion(Build {
            confidence: None,
            ..owned_by("a", "team-blue", AuthorityTier::Primary, 10)
        });
        let out = reconcile(&[winner], ts(50), &AuthorityPolicy::personal_default()).unwrap();
        assert_eq!(out.beliefs[0].confidence, 1.0);
    }

    // --- T4: advisory contradiction on equal-authority tie ---

    #[test]
    fn equal_authority_disagreement_yields_one_contradiction_and_no_belief() {
        let a = assertion(owned_by("a", "team-blue", AuthorityTier::Primary, 10));
        let b = assertion(owned_by("b", "team-red", AuthorityTier::Primary, 20));
        let out = reconcile(&[a, b], ts(50), &AuthorityPolicy::personal_default()).unwrap();
        assert_eq!(out.beliefs.len(), 0);
        assert_eq!(out.contradictions.len(), 1);
        let c = &out.contradictions[0];
        assert_eq!(c.kind, ContradictionKind::Logical);
        assert_eq!(c.status, ContradictionStatus::Open);
        assert_eq!(c.targets.len(), 2);
        // severity is the max confidence over the conflicting records (both 0.9).
        assert!((c.severity - 0.9).abs() < f32::EPSILON);
        // reasoning reports the distinct-claim count, not just the assertion count.
        assert!(
            c.reasoning
                .as_deref()
                .unwrap()
                .contains("2 distinct claims"),
            "reasoning: {:?}",
            c.reasoning
        );
    }

    #[test]
    fn agreeing_top_tier_cites_highest_confidence_assertion() {
        // Three agreeing (same object) Primary assertions; the belief must cite
        // the highest-confidence one — guards the pick_winner tie-break.
        let low = assertion(Build {
            confidence: Some(0.3),
            ..owned_by("a-low", "team-blue", AuthorityTier::Primary, 30)
        });
        let high = assertion(Build {
            confidence: Some(0.95),
            ..owned_by("b-high", "team-blue", AuthorityTier::Primary, 10)
        });
        let mid = assertion(Build {
            confidence: Some(0.6),
            ..owned_by("c-mid", "team-blue", AuthorityTier::Primary, 20)
        });
        let out = reconcile(
            &[low, high, mid],
            ts(50),
            &AuthorityPolicy::personal_default(),
        )
        .unwrap();
        assert_eq!(out.beliefs.len(), 1);
        assert_eq!(out.contradictions.len(), 0);
        assert_eq!(out.beliefs[0].sources[0].target_id, "b-high");
    }

    #[test]
    fn tier_absent_from_policy_ranks_below_listed_tiers() {
        // Policy lists only Primary; a Secondary assertion is unlisted and must
        // rank below the listed Primary — guards rank()'s unlisted-tier fallback.
        let primary = assertion(owned_by("a", "team-blue", AuthorityTier::Primary, 10));
        let secondary = assertion(owned_by("b", "team-red", AuthorityTier::Secondary, 20));
        let policy = AuthorityPolicy {
            tiers_high_to_low: vec![AuthorityTier::Primary],
            tie: TieRule::ContradictionOnTie,
        };
        let out = reconcile(&[primary, secondary], ts(50), &policy).unwrap();
        assert_eq!(out.beliefs.len(), 1);
        assert_eq!(out.contradictions.len(), 0);
        assert!(out.beliefs[0].content.contains("team-blue"));
    }

    #[test]
    fn reconcile_does_not_mutate_inputs() {
        let a = assertion(owned_by("a", "team-blue", AuthorityTier::Primary, 10));
        let b = assertion(owned_by("b", "team-red", AuthorityTier::Primary, 20));
        let inputs = vec![a, b];
        let before = inputs.clone();
        let _ = reconcile(&inputs, ts(50), &AuthorityPolicy::personal_default()).unwrap();
        assert_eq!(inputs, before);
    }

    #[test]
    fn equal_authority_with_disjoint_validity_does_not_contradict() {
        // team-blue valid [0,20); team-red valid [20,∞). At t=30 only red is live.
        let earlier = assertion(Build {
            valid_from: Some(0),
            valid_until: Some(20),
            ..owned_by("a", "team-blue", AuthorityTier::Primary, 5)
        });
        let later = assertion(Build {
            valid_from: Some(20),
            valid_until: None,
            ..owned_by("b", "team-red", AuthorityTier::Primary, 5)
        });
        let out = reconcile(
            &[earlier, later],
            ts(30),
            &AuthorityPolicy::personal_default(),
        )
        .unwrap();
        assert_eq!(out.contradictions.len(), 0);
        assert_eq!(out.beliefs.len(), 1);
        assert!(out.beliefs[0].content.contains("team-red"));
    }

    #[test]
    fn higher_confidence_does_not_resolve_a_same_tier_disagreement() {
        let low = assertion(Build {
            confidence: Some(0.2),
            ..owned_by("a", "team-blue", AuthorityTier::Primary, 10)
        });
        let high = assertion(Build {
            confidence: Some(0.99),
            ..owned_by("b", "team-red", AuthorityTier::Primary, 10)
        });
        let out = reconcile(&[low, high], ts(50), &AuthorityPolicy::personal_default()).unwrap();
        assert_eq!(out.beliefs.len(), 0);
        assert_eq!(out.contradictions.len(), 1);
    }

    // --- id encoding: collision safety across component boundaries ---

    #[test]
    fn record_id_disambiguates_colon_bearing_components() {
        let s = scope_key(&scope());
        // Without length-prefixing these would both be "belief:{s}:a:b:c".
        assert_ne!(
            record_id("belief", &[&s, "a:b", "c"]),
            record_id("belief", &[&s, "a", "b:c"]),
        );
    }

    #[test]
    fn scope_key_disambiguates_separator_bearing_fields() {
        let mut a = scope();
        a.tenant = "a|b".to_owned();
        let mut b = scope();
        b.tenant = "a".to_owned();
        b.workspace = Some("b".to_owned());
        assert_ne!(scope_key(&a), scope_key(&b));
    }
}
