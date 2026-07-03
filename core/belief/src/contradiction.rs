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
