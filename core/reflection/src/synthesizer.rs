//! Reflection synthesizer — implements `BeliefSynthesizer` (the empty slot).
//!
//! Reads a scope's active memories via [`ActiveMemorySource`] and produces a
//! deterministic reflection-summary belief. The real LLM insight-synthesis is
//! deferred behind the same trait (feature-gated adapter, follow-up).

use std::sync::Arc;

use async_trait::async_trait;
use engram_belief::BeliefSynthesizer;
use engram_domain::{Belief, ConsolidationRequest, Timestamp};
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
