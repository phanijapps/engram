//! Contradiction target canonicalization.
//!
//! Compatibility stores often need idempotent pair insertion. Canonicalizing
//! target pairs here keeps that rule deterministic without making contradiction
//! resolution mutate either target.

use engram_domain::{ContradictionTarget, ContradictionTargetType};

/// A two-target contradiction key in deterministic order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalContradictionPair {
    pub left: ContradictionTarget,
    pub right: ContradictionTarget,
}

/// Returns a deterministic target order for pair idempotency.
pub fn canonicalize_pair(
    first: ContradictionTarget,
    second: ContradictionTarget,
) -> CanonicalContradictionPair {
    if target_key(&first) <= target_key(&second) {
        CanonicalContradictionPair {
            left: first,
            right: second,
        }
    } else {
        CanonicalContradictionPair {
            left: second,
            right: first,
        }
    }
}

/// Builds a stable pair key suitable for adapter idempotency indexes.
pub fn canonical_pair_key(first: &ContradictionTarget, second: &ContradictionTarget) -> String {
    let pair = canonicalize_pair(first.clone(), second.clone());
    format!("{}|{}", target_key(&pair.left), target_key(&pair.right))
}

fn target_key(target: &ContradictionTarget) -> String {
    format!(
        "{}:{}:{}",
        target_type_key(&target.target_type),
        target.target_id,
        target.role.as_deref().unwrap_or("")
    )
}

fn target_type_key(target_type: &ContradictionTargetType) -> &'static str {
    match target_type {
        ContradictionTargetType::Belief => "belief",
        ContradictionTargetType::Memory => "memory",
        ContradictionTargetType::Assertion => "assertion",
        ContradictionTargetType::Chunk => "chunk",
        ContradictionTargetType::Entity => "entity",
        ContradictionTargetType::Relationship => "relationship",
        ContradictionTargetType::Concept => "concept",
    }
}

#[cfg(test)]
mod tests {
    use engram_domain::ContradictionTargetType;

    use super::*;

    fn target(target_type: ContradictionTargetType, id: &str) -> ContradictionTarget {
        ContradictionTarget {
            target_type,
            target_id: id.to_owned(),
            role: None,
        }
    }

    #[test]
    fn canonical_pair_key_is_order_independent() {
        let memory = target(ContradictionTargetType::Memory, "m-1");
        let belief = target(ContradictionTargetType::Belief, "b-1");

        assert_eq!(
            canonical_pair_key(&memory, &belief),
            canonical_pair_key(&belief, &memory)
        );
        let pair = canonicalize_pair(memory, belief);
        assert_eq!(pair.left.target_type, ContradictionTargetType::Belief);
        assert_eq!(pair.right.target_type, ContradictionTargetType::Memory);
    }
}

// Advisory detector shared by all recipes; persistence remains adapter-owned.
use engram_domain::*;
use engram_runtime::CoreResult;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
/// Existing active-subject contradiction algorithm with an injected clock value.
/// Callers supply one governed scope's belief set.
pub fn detect_belief_contradictions(
    beliefs: &[Belief],
    now: Timestamp,
) -> CoreResult<Vec<Contradiction>> {
    let mut groups: HashMap<String, Vec<&Belief>> = HashMap::new();
    for belief in beliefs {
        if belief.status == BeliefStatus::Active {
            groups
                .entry(belief.subject.key.clone())
                .or_default()
                .push(belief);
        }
    }

    let mut findings = Vec::new();
    for (key, group) in groups {
        if group.len() < 2 {
            continue;
        }
        let distinct: HashSet<&str> = group.iter().map(|b| b.content.as_str()).collect();
        if distinct.len() < 2 {
            continue;
        }
        let severity = group
            .iter()
            .map(|b| b.confidence)
            .fold(0.0_f32, f32::max)
            .clamp(0.0, 1.0);
        let targets = group
            .iter()
            .map(|b| ContradictionTarget {
                target_type: ContradictionTargetType::Belief,
                target_id: b.id.to_string(),
                role: None,
            })
            .collect::<Vec<_>>();
        findings.push(Contradiction {
            id: contradiction_id_for(&key),
            scope: group[0].scope.clone(),
            kind: ContradictionKind::Logical,
            targets,
            severity,
            status: ContradictionStatus::Open,
            reasoning: Some(format!(
                "{} active beliefs on `{key}` disagree",
                group.len()
            )),
            detected_by: None,
            resolution: None,
            provenance: detector_provenance(&key, now),
            detected_at: now,
            updated_at: None,
        });
    }
    findings.sort_by_key(|f| f.id.to_string());
    Ok(findings)
}
fn contradiction_id_for(key: &str) -> ContradictionId {
    let hash = Sha256::digest(key.as_bytes());
    ContradictionId::from(format!("contradiction-{}", hex(&hash[..8])))
}

fn detector_provenance(key: &str, now: Timestamp) -> Provenance {
    Provenance {
        source: format!("belief-detector:{key}"),
        actor: Actor {
            id: Id::from("engram-contradiction-detector"),
            kind: ActorKind::System,
            display_name: Some("Contradiction detector".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("contradiction_detection".to_owned()),
    }
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod detector_contracts {
    use super::*;
    fn belief(id: &str, content: &str) -> Belief {
        serde_json::from_value(serde_json::json!({"id":id,"scope":{"tenant":"detector-scope"},"subject":{"key":"shared-subject"},"kind":"assessment","status":"active","content":content,"confidence":0.75,"provenance":{"source":"detector-contract","actor":{"id":"actor","kind":"agent"},"observedAt":"2026-11-17T00:00:00Z"},"policy":{"visibility":"private","retention":"durable"},"createdAt":"2026-11-17T00:00:00Z","validFrom":"2026-11-17T00:00:00Z"})).unwrap()
    }
    #[test]
    fn existing_detector_grouping_ids_lifecycle_clock_and_order_are_preserved() {
        let now: Timestamp = "2026-11-17T01:00:00Z".parse().unwrap();
        let rows = vec![belief("one", "first"), belief("two", "second")];
        let result = detect_belief_contradictions(&rows, now).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].detected_at, now);
        assert_eq!(result[0].status, ContradictionStatus::Open);
        assert_eq!(result[0].severity, 0.75);
        assert_eq!(result[0].targets.len(), 2);
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(
            detect_belief_contradictions(&reversed, now).unwrap()[0].id,
            result[0].id
        );
        let mut inactive = rows.clone();
        inactive[1].status = BeliefStatus::Retracted;
        assert!(
            detect_belief_contradictions(&inactive, now)
                .unwrap()
                .is_empty()
        );
        assert!(
            detect_belief_contradictions(&[belief("one", "same"), belief("two", "same")], now)
                .unwrap()
                .is_empty()
        );
    }
}
