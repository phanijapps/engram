//! Reflection synthesizer — implements `BeliefSynthesizer` (the empty slot).
//!
//! Reads a scope's active memories via [`ActiveMemorySource`] and produces a
//! deterministic reflection-summary belief. The real LLM insight-synthesis is
//! deferred behind the same trait (feature-gated adapter, follow-up).

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefSynthesizer;
use engram_domain::{
    Actor, ActorKind, Belief, BeliefId, BeliefStatus, BeliefSubject, ConsolidationRequest,
    DerivationKind, DerivationRef, Id, Provenance, Timestamp,
};
use engram_runtime::CoreResult;

use crate::belief_build::reflection_belief;
use crate::source::ActiveMemorySource;

/// Reflection synthesizer: abstracts scoped active memories into derived beliefs.
///
/// Holds an [`ActiveMemorySource`] (the narrow read port) and a fixed timestamp
/// (for deterministic output). The deterministic baseline produces one summary
/// belief; the real LLM impl replaces this behind the same `BeliefSynthesizer`
/// trait.
pub struct ReflectionSynthesizer {
    source: Arc<dyn ActiveMemorySource>,
    now: Timestamp,
}

impl ReflectionSynthesizer {
    /// Creates a reflection synthesizer with the given memory source + timestamp.
    pub fn new(source: Arc<dyn ActiveMemorySource>, now: Timestamp) -> Self {
        Self { source, now }
    }
}

#[async_trait]
impl BeliefSynthesizer for ReflectionSynthesizer {
    async fn synthesize_beliefs(&self, request: &ConsolidationRequest) -> CoreResult<Vec<Belief>> {
        let evidence = self.source.active_memories(&request.scope).await?;
        if evidence.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![reflection_belief(&evidence, &request.scope, self.now)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{Scope, Timestamp};
    use futures::executor::block_on;

    struct StubSource {
        evidence: Vec<crate::source::ActiveMemoryEvidence>,
    }

    fn evidence_of(id: &str, text: &str) -> crate::source::ActiveMemoryEvidence {
        crate::source::ActiveMemoryEvidence {
            memory_id: engram_domain::MemoryId::from(id),
            text: text.to_owned(),
            policy: engram_domain::Policy {
                visibility: engram_domain::Visibility::Workspace,
                retention: engram_domain::Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            observed_at: chrono::Utc::now(),
        }
    }

    #[async_trait]
    impl ActiveMemorySource for StubSource {
        async fn active_memories(
            &self,
            _scope: &Scope,
        ) -> CoreResult<Vec<crate::source::ActiveMemoryEvidence>> {
            Ok(self.evidence.clone())
        }
    }

    fn now() -> Timestamp {
        chrono::Utc::now()
    }

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn request() -> ConsolidationRequest {
        // Minimal request — only scope is read by the synthesizer.
        ConsolidationRequest {
            scope: scope(),
            requester: engram_domain::Requester {
                actor: engram_domain::Actor {
                    id: engram_domain::Id::from("reflection-test"),
                    kind: engram_domain::ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            since: None,
            until: None,
            strategy: None,
            dry_run: None,
        }
    }

    #[test]
    fn produces_reflection_belief_from_active_memories() {
        let source = Arc::new(StubSource {
            evidence: vec!["Alice likes cats".to_owned(), "Bob likes dogs".to_owned()]
                .into_iter()
                .enumerate()
                .map(|(i, t)| evidence_of(&format!("m-{i}"), &t))
                .collect(),
        });
        let synth = ReflectionSynthesizer::new(source, now());
        let beliefs = block_on(synth.synthesize_beliefs(&request())).unwrap();
        assert_eq!(beliefs.len(), 1);
        let b = &beliefs[0];
        assert_eq!(b.status, engram_domain::BeliefStatus::Active);
        assert_eq!(b.provenance.method.as_deref(), Some("reflection"));
        assert_eq!(b.provenance.source, "reflection-synthesizer");
        assert!(b.content.contains("Alice"));
        assert!(b.content.contains("Bob"));
        assert!(b.reasoning.as_ref().unwrap().contains("2"));
    }

    #[test]
    fn empty_active_memories_produces_no_beliefs() {
        let source = Arc::new(StubSource {
            evidence: Vec::new(),
        });
        let synth = ReflectionSynthesizer::new(source, now());
        let beliefs = block_on(synth.synthesize_beliefs(&request())).unwrap();
        assert!(beliefs.is_empty());
    }

    #[test]
    fn single_memory_content_is_the_text_itself() {
        let source = Arc::new(StubSource {
            evidence: vec!["solo insight".to_owned()]
                .into_iter()
                .enumerate()
                .map(|(i, t)| evidence_of(&format!("m-{i}"), &t))
                .collect(),
        });
        let synth = ReflectionSynthesizer::new(source, now());
        let beliefs = block_on(synth.synthesize_beliefs(&request())).unwrap();
        assert_eq!(beliefs.len(), 1);
        assert_eq!(beliefs[0].content, "solo insight");
    }
}

/// Pattern-based belief synthesis: extracts recurring topics from the texts
/// and creates one belief per topic (instead of one concatenated blob).
///
/// NOTE: imports are at the top of this file (shared with ReflectionSynthesizer).
///
/// Phase 2.1 enhancement: the deterministic blob synthesizer created one
/// belief containing ALL texts joined with "; ". This creates one belief
/// per detected topic, so `beliefs op=get subject=<topic>` finds the
/// relevant belief without parsing a wall of text.
///
/// Topic detection: the first 2-4 words of each text (up to the first ':'
/// or '(') form the topic key. Texts with the same topic key are grouped
/// into one belief. Deterministic, no LLM.
pub struct PatternSynthesizer {
    source: Arc<dyn ActiveMemorySource>,
    now: Timestamp,
}

impl PatternSynthesizer {
    pub fn new(source: Arc<dyn ActiveMemorySource>, now: Timestamp) -> Self {
        Self { source, now }
    }
}

/// Extracts a topic key from the first line/heading of a text.
/// "GOTCHAS (each cost...): ..." → "gotchas"
/// "ENG ARCHITECTURE: ..." → "eng-architecture"
/// "TOOL SURFACE: ..." → "tool-surface"
/// Unknown/random text → "general"
fn topic_key(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("");
    let header = first_line
        .split([':', '('])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if header.is_empty() || header.len() < 3 {
        return "general".to_owned();
    }
    // Normalize: alphanumerics + hyphens only
    let normalized: String = header
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse consecutive hyphens and trim
    let mut collapsed = String::new();
    let mut prev_hyphen = false;
    for c in normalized.chars() {
        if c == '-' {
            if !prev_hyphen && !collapsed.is_empty() {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }
    let key = collapsed.trim_matches('-').to_owned();
    if key.len() < 3 || key.len() > 60 {
        "general".to_owned()
    } else {
        key
    }
}

#[async_trait]
impl BeliefSynthesizer for PatternSynthesizer {
    async fn synthesize_beliefs(&self, request: &ConsolidationRequest) -> CoreResult<Vec<Belief>> {
        let evidence = self.source.active_memories(&request.scope).await?;
        if evidence.is_empty() {
            return Ok(Vec::new());
        }

        // Group evidence by topic (lineage travels with each item).
        let mut topics: std::collections::BTreeMap<
            String,
            Vec<&crate::source::ActiveMemoryEvidence>,
        > = std::collections::BTreeMap::new();
        for item in &evidence {
            let key = topic_key(&item.text);
            topics.entry(key).or_default().push(item);
        }

        // One belief per topic
        let mut beliefs = Vec::new();
        for (topic, topic_texts) in topics {
            if topic_texts.is_empty() {
                continue;
            }
            // Lineage (MCCL-audit F2): cite every input memory in sources
            // AND derivation input_refs; inherit the never-widen policy.
            let sources: Vec<engram_domain::BeliefSource> = topic_texts
                .iter()
                .map(|item| engram_domain::BeliefSource {
                    target_type: engram_domain::BeliefSourceTargetType::Memory,
                    target_id: item.memory_id.to_string(),
                    authority_level: None,
                    weight: None,
                    confidence: None,
                    valid_from: Some(item.observed_at),
                    valid_until: None,
                })
                .collect();
            let input_refs: Vec<engram_domain::EvidenceRef> = topic_texts
                .iter()
                .map(|item| engram_domain::EvidenceRef {
                    target_type: engram_domain::EvidenceTargetType::Memory,
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
                created_at: self.now,
            };
            let content = if topic_texts.len() == 1 {
                topic_texts[0].text.clone()
            } else {
                let joined = topic_texts
                    .iter()
                    .map(|item| item.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("{} memories on '{}': {}", topic_texts.len(), topic, joined)
            };
            let content_hash = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                topic.hash(&mut hasher);
                for item in &topic_texts {
                    item.memory_id.to_string().hash(&mut hasher);
                    item.text.hash(&mut hasher);
                }
                hasher.finish()
            };
            let composed_policy = engram_domain::compose_derived_policy(
                &topic_texts
                    .iter()
                    .map(|item| item.policy.clone())
                    .collect::<Vec<_>>(),
            );
            beliefs.push(Belief {
                id: BeliefId::from(Id::from(format!("reflection:{}:{:x}", topic, content_hash))),
                scope: request.scope.clone(),
                subject: BeliefSubject {
                    key: format!("reflection:{}", topic),
                    entity_ref: None,
                    concept_ref: None,
                    aliases: Vec::new(),
                },
                content,
                confidence: 0.7,
                status: BeliefStatus::Active,
                sources,
                valid_from: Some(self.now),
                valid_until: None,
                synthesizer: Some(derivation.clone()),
                reasoning: Some(format!(
                    "pattern-based reflection: {} memory texts grouped by topic '{}'",
                    topic_texts.len(),
                    topic
                )),
                embedding_refs: Vec::new(),
                policy: composed_policy,
                provenance: Provenance {
                    source: "engram-reflection".to_owned(),
                    actor: Actor {
                        id: Id::from("engram-reflection"),
                        kind: ActorKind::Agent,
                        display_name: None,
                        metadata: None,
                    },
                    observed_at: self.now,
                    evidence: Vec::new(),
                    derivations: vec![derivation],
                    confidence: Some(0.7),
                    method: Some("pattern-reflection".to_owned()),
                },
                created_at: self.now,
                updated_at: None,
                metadata: None,
                stale: None,
                superseded_by: None,
            });
        }

        Ok(beliefs)
    }
}

#[cfg(test)]
mod pattern_tests {
    use super::*;
    use engram_domain::Scope;
    use futures::executor::block_on;

    struct StubSource {
        evidence: Vec<crate::source::ActiveMemoryEvidence>,
    }

    fn evidence_of(id: &str, text: &str) -> crate::source::ActiveMemoryEvidence {
        crate::source::ActiveMemoryEvidence {
            memory_id: engram_domain::MemoryId::from(id),
            text: text.to_owned(),
            policy: engram_domain::Policy {
                visibility: engram_domain::Visibility::Workspace,
                retention: engram_domain::Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            observed_at: chrono::Utc::now(),
        }
    }

    #[async_trait]
    impl ActiveMemorySource for StubSource {
        async fn active_memories(
            &self,
            _scope: &Scope,
        ) -> CoreResult<Vec<crate::source::ActiveMemoryEvidence>> {
            Ok(self.evidence.clone())
        }
    }

    #[test]
    fn topic_key_extracts_headers() {
        assert_eq!(topic_key("GOTCHAS (each cost time): (1) SQLite"), "gotchas");
        assert_eq!(
            topic_key("ENG ARCHITECTURE: Contract-first"),
            "eng-architecture"
        );
        assert_eq!(topic_key("TOOL SURFACE: 6 core tools"), "tool-surface");
        assert_eq!(
            topic_key("random text without header"),
            "random-text-without-header"
        );
    }

    #[test]
    fn pattern_synthesizer_groups_by_topic() {
        let source = Arc::new(StubSource {
            evidence: vec![
                "GOTCHAS: camelCase paths".to_owned(),
                "GOTCHAS: scope strictness".to_owned(),
                "ARCHITECTURE: 8 domain concepts".to_owned(),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, t)| evidence_of(&format!("m-{i}"), &t))
            .collect(),
        });
        let now = chrono::Utc::now();
        let synth = PatternSynthesizer::new(source, now);
        let scope = Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: Some("w".to_owned()),
            session: None,
            environment: None,
        };
        let request = ConsolidationRequest {
            scope,
            requester: engram_domain::Requester {
                actor: Actor {
                    id: Id::from("pattern-synthesizer"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            since: None,
            until: None,
            strategy: None,
            dry_run: Some(false),
        };
        let beliefs = block_on(async { synth.synthesize_beliefs(&request).await }).unwrap();
        assert_eq!(
            beliefs.len(),
            2,
            "2 topics (gotchas + architecture): {beliefs:?}"
        );
        let gotchas = beliefs
            .iter()
            .find(|b| b.subject.key.contains("gotchas"))
            .unwrap();
        assert!(gotchas.content.contains("camelCase"));
        assert!(gotchas.content.contains("scope strictness"));
    }
}

#[cfg(test)]
mod lineage_tests {
    use super::*;
    use crate::source::{ActiveMemoryEvidence, ActiveMemorySource};
    use engram_domain::ConsolidationRequest;
    use engram_domain::Scope;
    use engram_domain::{Actor, ActorKind, Id, Requester};
    use engram_domain::{
        AllowedUse, BeliefSourceTargetType, MemoryId, Policy, Retention, Sensitivity, Visibility,
    };
    use engram_runtime::CoreResult;
    use futures::executor::block_on;
    use std::sync::Arc;

    fn evidence(id: &str, text: &str, policy: Policy) -> ActiveMemoryEvidence {
        ActiveMemoryEvidence {
            memory_id: MemoryId::from(id),
            text: text.to_owned(),
            policy,
            observed_at: chrono::Utc::now(),
        }
    }

    fn open_policy() -> Policy {
        Policy {
            visibility: Visibility::Organization,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval, AllowedUse::Consolidation],
            expires_at: None,
            delete_mode: None,
        }
    }

    fn restricted_policy() -> Policy {
        Policy {
            visibility: Visibility::Private,
            retention: Retention::Session,
            sensitivity: Some(Sensitivity::High),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        }
    }

    struct MixedSource(Vec<ActiveMemoryEvidence>);

    #[allow(clippy::double_must_use)]
    #[async_trait]
    impl ActiveMemorySource for MixedSource {
        async fn active_memories(&self, _scope: &Scope) -> CoreResult<Vec<ActiveMemoryEvidence>> {
            Ok(self.0.clone())
        }
    }

    fn request() -> ConsolidationRequest {
        ConsolidationRequest {
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            requester: Requester {
                actor: Actor {
                    id: Id::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: vec![],
                permissions: vec![],
                on_behalf_of: None,
            },
            since: None,
            until: None,
            strategy: None,
            dry_run: Some(false),
        }
    }

    /// MCCL-audit F2 regression: derived beliefs CITE every input (sources +
    /// derivation input_refs) and NEVER widen policy below the most
    /// restrictive input.
    #[test]
    fn derived_beliefs_cite_inputs_and_never_widen_policy() {
        let source = Arc::new(MixedSource(vec![
            evidence("mem-open", "GOTCHAS: careful with budgets", open_policy()),
            evidence(
                "mem-restricted",
                "GOTCHAS: secrets stay private",
                restricted_policy(),
            ),
        ]));
        let synth = PatternSynthesizer::new(source, chrono::Utc::now());
        let beliefs = block_on(synth.synthesize_beliefs(&request())).expect("synthesize");

        assert_eq!(beliefs.len(), 1, "both texts share the GOTCHAS topic");
        let belief = &beliefs[0];

        // Lineage: every input memory is cited in sources AND input_refs.
        let source_ids: Vec<&str> = belief
            .sources
            .iter()
            .map(|s| s.target_id.as_str())
            .collect();
        assert!(source_ids.contains(&"mem-open"));
        assert!(source_ids.contains(&"mem-restricted"));
        assert!(
            belief
                .sources
                .iter()
                .all(|s| s.target_type == BeliefSourceTargetType::Memory)
        );
        let input_ref_ids: Vec<String> = belief
            .synthesizer
            .as_ref()
            .map(|d| {
                d.input_refs
                    .iter()
                    .filter_map(|r| r.target_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            input_ref_ids.len(),
            2,
            "derivation input_refs cite both inputs"
        );

        // Never-widen policy: most restrictive input won every dimension.
        assert_eq!(belief.policy.visibility, Visibility::Private);
        assert_eq!(belief.policy.retention, Retention::Session);
        assert_eq!(belief.policy.sensitivity, Some(Sensitivity::High));
        assert_eq!(belief.policy.allowed_uses, vec![AllowedUse::Retrieval]);
    }

    /// Determinism: same evidence → same belief id; different memory ids with
    /// same text → different ids (identity participates in the hash).
    #[test]
    fn belief_identity_is_deterministic_and_identity_sensitive() {
        let evs = vec![evidence("mem-1", "GOTCHAS: alpha", open_policy())];
        let a = {
            let synth =
                PatternSynthesizer::new(Arc::new(MixedSource(evs.clone())), chrono::Utc::now());
            block_on(synth.synthesize_beliefs(&request()))
                .unwrap()
                .remove(0)
        };
        let b = {
            let synth =
                PatternSynthesizer::new(Arc::new(MixedSource(evs.clone())), chrono::Utc::now());
            block_on(synth.synthesize_beliefs(&request()))
                .unwrap()
                .remove(0)
        };
        assert_eq!(a.id, b.id, "same evidence -> same id");

        let other = {
            let synth = PatternSynthesizer::new(
                Arc::new(MixedSource(vec![evidence(
                    "mem-2",
                    "GOTCHAS: alpha",
                    open_policy(),
                )])),
                chrono::Utc::now(),
            );
            block_on(synth.synthesize_beliefs(&request()))
                .unwrap()
                .remove(0)
        };
        assert_ne!(
            a.id, other.id,
            "same text, different memory -> different id"
        );
    }
}
