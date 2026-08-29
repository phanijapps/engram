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
        let texts = self.source.active_memory_texts(&request.scope).await?;
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![reflection_belief(&texts, &request.scope, self.now)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{Scope, Timestamp};
    use futures::executor::block_on;

    struct StubSource {
        texts: Vec<String>,
    }

    #[async_trait]
    impl ActiveMemorySource for StubSource {
        async fn active_memory_texts(&self, _scope: &Scope) -> CoreResult<Vec<String>> {
            Ok(self.texts.clone())
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
            texts: vec!["Alice likes cats".to_owned(), "Bob likes dogs".to_owned()],
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
        let source = Arc::new(StubSource { texts: Vec::new() });
        let synth = ReflectionSynthesizer::new(source, now());
        let beliefs = block_on(synth.synthesize_beliefs(&request())).unwrap();
        assert!(beliefs.is_empty());
    }

    #[test]
    fn single_memory_content_is_the_text_itself() {
        let source = Arc::new(StubSource {
            texts: vec!["solo insight".to_owned()],
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
        let texts = self.source.active_memory_texts(&request.scope).await?;
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Group texts by topic
        let mut topics: std::collections::BTreeMap<String, Vec<&str>> =
            std::collections::BTreeMap::new();
        for text in &texts {
            let key = topic_key(text);
            topics.entry(key).or_default().push(text);
        }

        // One belief per topic
        let mut beliefs = Vec::new();
        for (topic, topic_texts) in topics {
            if topic_texts.is_empty() {
                continue;
            }
            let derivation = DerivationRef {
                kind: DerivationKind::Consolidation,
                model: None,
                prompt_hash: None,
                input_refs: Vec::new(),
                created_at: self.now,
            };
            let content = if topic_texts.len() == 1 {
                topic_texts[0].to_owned()
            } else {
                format!(
                    "{} memories on '{}': {}",
                    topic_texts.len(),
                    topic,
                    topic_texts.join("; ")
                )
            };
            let content_hash = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                topic.hash(&mut hasher);
                content.hash(&mut hasher);
                hasher.finish()
            };
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
                sources: Vec::new(),
                valid_from: Some(self.now),
                valid_until: None,
                synthesizer: Some(derivation.clone()),
                reasoning: Some(format!(
                    "pattern-based reflection: {} memory texts grouped by topic '{}'",
                    topic_texts.len(),
                    topic
                )),
                embedding_refs: Vec::new(),
                policy: engram_domain::Policy {
                    visibility: engram_domain::Visibility::Workspace,
                    retention: engram_domain::Retention::Durable,
                    sensitivity: Some(engram_domain::Sensitivity::Low),
                    allowed_uses: Vec::new(),
                    expires_at: None,
                    delete_mode: None,
                },
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
        texts: Vec<String>,
    }

    #[async_trait]
    impl ActiveMemorySource for StubSource {
        async fn active_memory_texts(&self, _scope: &Scope) -> CoreResult<Vec<String>> {
            Ok(self.texts.clone())
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
            texts: vec![
                "GOTCHAS: camelCase paths".to_owned(),
                "GOTCHAS: scope strictness".to_owned(),
                "ARCHITECTURE: 8 domain concepts".to_owned(),
            ],
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
