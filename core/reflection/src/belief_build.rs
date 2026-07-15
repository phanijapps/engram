//! Pure helper: build a reflection-derived `Belief` from active memory texts.
//!
//! Mirrors the construction pattern of `core/belief/src/reconcile.rs::build_belief`
//! but produces a deterministic reflection-summary belief tagged
//! `provenance.method = "reflection"`.

use engram_domain::{
    Actor, ActorKind, Belief, BeliefId, BeliefStatus, BeliefSubject, DerivationKind, DerivationRef,
    Id, Policy, Provenance, Retention, Scope, Sensitivity, Timestamp, Visibility,
};

/// The provenance source string stamped on every reflection-derived belief.
pub(crate) const SOURCE: &str = "reflection-synthesizer";

/// Builds a single deterministic reflection-summary belief over `texts`.
///
/// The belief's `content` concatenates the active memory texts; `reasoning`
/// records the count; `provenance.method = "reflection"` distinguishes it from
/// future bottom-up belief synthesis without a contract change.
pub(crate) fn reflection_belief(texts: &[String], scope: &Scope, now: Timestamp) -> Belief {
    let derivation = DerivationRef {
        kind: DerivationKind::Consolidation,
        model: None,
        prompt_hash: None,
        input_refs: Vec::new(),
        created_at: now,
    };
    let content_hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        texts.hash(&mut hasher);
        hasher.finish()
    };
    let summary = if texts.len() == 1 {
        texts[0].clone()
    } else {
        format!("{} active memories: {}", texts.len(), texts.join("; "))
    };
    Belief {
        id: BeliefId::from(Id::from(format!(
            "reflection-{}-{:x}",
            scope.tenant, content_hash
        ))),
        scope: scope.clone(),
        subject: BeliefSubject {
            key: format!("reflection:{:x}", content_hash),
            entity_ref: None,
            concept_ref: None,
            aliases: Vec::new(),
        },
        content: summary,
        status: BeliefStatus::Active,
        confidence: 0.5,
        sources: Vec::new(),
        valid_from: Some(now),
        valid_until: None,
        superseded_by: None,
        stale: None,
        synthesizer: Some(derivation.clone()),
        reasoning: Some(format!(
            "deterministic baseline reflection over {} active memory/memories",
            texts.len()
        )),
        embedding_refs: Vec::new(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: Vec::new(),
            expires_at: None,
            delete_mode: None,
        },
        provenance: Provenance {
            source: SOURCE.to_owned(),
            actor: Actor {
                id: Id::from("reflection-synthesizer"),
                kind: ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: now,
            evidence: Vec::new(),
            derivations: vec![derivation],
            confidence: Some(0.5),
            method: Some("reflection".to_owned()),
        },
        created_at: now,
        updated_at: None,
        metadata: None,
    }
}
