//! Pure helper: build a reflection-derived `Belief` from active memory texts.
//!
//! Mirrors the construction pattern of `core/belief/src/reconcile.rs::build_belief`
//! but produces a deterministic reflection-summary belief tagged
//! `provenance.method = "reflection"`.

use engram_domain::{
    Actor, ActorKind, Belief, BeliefId, BeliefSource, BeliefSourceTargetType, BeliefStatus,
    BeliefSubject, DerivationKind, DerivationRef, EvidenceRef, EvidenceTargetType, Id, Provenance,
    Scope, Timestamp, compose_derived_policy,
};

use crate::source::ActiveMemoryEvidence;

/// The provenance source string stamped on every reflection-derived belief.
pub(crate) const SOURCE: &str = "reflection-synthesizer";

/// Builds a single deterministic reflection-summary belief over active-memory
/// evidence, PRESERVING LINEAGE (MCCL-audit F2): every input memory is cited
/// in `Belief.sources` and `DerivationRef.input_refs`, and the belief's policy
/// is the never-widen composition of its inputs' policies (a derived record
/// can never be less protected than its most restrictive input).
pub(crate) fn reflection_belief(
    evidence: &[ActiveMemoryEvidence],
    scope: &Scope,
    now: Timestamp,
) -> Belief {
    // Citations: `Belief.sources` (typed belief-source records) AND the
    // derivation's `input_refs` (evidence refs) both resolve every input.
    let sources: Vec<BeliefSource> = evidence
        .iter()
        .map(|item| BeliefSource {
            target_type: BeliefSourceTargetType::Memory,
            target_id: item.memory_id.to_string(),
            authority_level: None,
            weight: None,
            confidence: None,
            valid_from: Some(item.observed_at),
            valid_until: None,
        })
        .collect();
    let input_refs: Vec<EvidenceRef> = evidence
        .iter()
        .map(|item| EvidenceRef {
            target_type: EvidenceTargetType::Memory,
            target_id: Some(item.memory_id.to_string()),
            uri: None,
            quote: None,
            location: None,
        })
        .collect();
    let derivation = DerivationRef {
        kind: DerivationKind::Consolidation,
        model: None,
        prompt_hash: None,
        input_refs: input_refs.clone(),
        created_at: now,
    };
    let content_hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        // Identity participates in the hash: same texts from different
        // memories are different derivations.
        for item in evidence {
            item.memory_id.to_string().hash(&mut hasher);
            item.text.hash(&mut hasher);
        }
        hasher.finish()
    };
    let texts: Vec<&str> = evidence.iter().map(|item| item.text.as_str()).collect();
    let summary = if texts.len() == 1 {
        texts[0].to_owned()
    } else {
        format!("{} active memories: {}", texts.len(), texts.join("; "))
    };
    let composed_policy = compose_derived_policy(
        &evidence
            .iter()
            .map(|item| item.policy.clone())
            .collect::<Vec<_>>(),
    );
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
        sources,
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
        policy: composed_policy,
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
