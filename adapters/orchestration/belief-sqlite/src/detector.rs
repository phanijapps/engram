//! Advisory contradiction detection for the SQLite belief adapter.
//!
//! Detection is deliberately separate from repository persistence. It reads a
//! caller-supplied belief slice, reports review records, and never mutates the
//! stored beliefs or contradictions it finds tension between.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::Utc;
use engram_core::ContradictionDetector;
use engram_domain::*;
use engram_runtime::CoreResult;
use sha2::{Digest, Sha256};

use crate::service::SqlBeliefStore;

#[async_trait]
impl ContradictionDetector for SqlBeliefStore {
    /// Groups active beliefs by subject and reports one logical contradiction
    /// for any subject whose active beliefs disagree on content.
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>> {
        let mut groups: HashMap<String, Vec<&Belief>> = HashMap::new();
        for belief in beliefs {
            if belief.status == BeliefStatus::Active {
                groups
                    .entry(belief.subject.key.clone())
                    .or_default()
                    .push(belief);
            }
        }

        let now = Utc::now();
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
        findings.sort_by(|left, right| left.id.to_string().cmp(&right.id.to_string()));
        Ok(findings)
    }
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
