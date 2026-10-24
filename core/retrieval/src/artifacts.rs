//! Code-artifact classification for retrieval result eligibility.
//!
//! Code scans persist structural entities alongside real symbols and
//! knowledge-graph nodes: import-path stubs (`vitest`,
//! `@testing-library/react`, `./FileEditor`) land as `Module` kind, file
//! stubs as `File`, repositories as `Repository`, and future scanners may
//! emit `Variable`. On real graphs these dominate retrieval output while
//! almost never being what a query wants (in the reference store they are
//! 37.5% of all entities).
//!
//! Suppression is **result eligibility only**: a suppressed entity still
//! seeds Personalized PageRank walks, still carries edges (concept → code
//! bridges such as `describes` / `realized_by`), still shapes community
//! structure, and still lives in shared graph snapshots. It is simply not
//! emitted as a ranked retrieval result unless the request opts in via
//! [`QueryFilter::include_code_artifacts`](engram_domain::QueryFilter).
//!
//! The predicate is kind-based on purpose — no name heuristics — so
//! knowledge-graph nodes (Concept, Api, Endpoint, Person, …) can never be
//! misclassified. A second, deliberately different kind set exists for
//! temporal analytics (`whats_changed` keeps `File`); the two must not be
//! merged.

use engram_domain::{EntityKind, KnowledgeEntity, RetrievalRequest};

/// Entity kinds suppressed from ranked retrieval results by default.
pub const CODE_ARTIFACT_KINDS: &[EntityKind] = &[
    EntityKind::Variable,
    EntityKind::Module,
    EntityKind::File,
    EntityKind::Repository,
];

/// True when the entity is a code artifact (import/file/repository stub or
/// variable declaration) — ineligible as a ranked retrieval result unless
/// the request opts in. Never true for knowledge-graph kinds.
pub fn is_code_artifact(entity: &KnowledgeEntity) -> bool {
    CODE_ARTIFACT_KINDS.contains(&entity.kind)
}

/// True when the request suppresses code artifacts (the default): the filter
/// is absent or explicitly `false`. Single source of truth for every
/// enforcement seam — lanes, resolvers, and tool-level injection paths all
/// consult this, never their own reading of the flag.
pub fn suppresses_code_artifacts(request: &RetrievalRequest) -> bool {
    request
        .filters
        .as_ref()
        .is_none_or(|f| f.include_code_artifacts != Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{Actor, ActorKind, Provenance, QueryFilter, Requester, Scope};

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn entity(kind: EntityKind) -> KnowledgeEntity {
        KnowledgeEntity {
            id: engram_domain::Id::from("e"),
            graph_id: None,
            kind,
            name: "stub".to_owned(),
            aliases: Vec::new(),
            scope: scope(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: engram_domain::Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".to_owned()),
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
            archived_at: None,
        }
    }

    #[test]
    fn classifies_exactly_the_artifact_kinds() {
        for kind in [
            EntityKind::Variable,
            EntityKind::Module,
            EntityKind::File,
            EntityKind::Repository,
        ] {
            assert!(
                is_code_artifact(&entity(kind.clone())),
                "{kind:?} is an artifact"
            );
        }
        for kind in [
            EntityKind::Function,
            EntityKind::Struct,
            EntityKind::Class,
            EntityKind::Interface,
            EntityKind::Concept,
            EntityKind::Api,
            EntityKind::Endpoint,
            EntityKind::Person,
        ] {
            assert!(
                !is_code_artifact(&entity(kind.clone())),
                "{kind:?} must never be suppressed"
            );
        }
    }

    fn request(filter: Option<QueryFilter>) -> RetrievalRequest {
        RetrievalRequest {
            query: "q".to_owned(),
            scope: scope(),
            requester: Requester {
                actor: Actor {
                    id: engram_domain::Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: Vec::new(),
            filters: filter,
            cues: Vec::new(),
            limit: None,
            budget: None,
            include_explanations: None,
        }
    }

    fn filter(include: Option<bool>) -> QueryFilter {
        QueryFilter {
            memory_kinds: Vec::new(),
            source_kinds: Vec::new(),
            chunk_kinds: Vec::new(),
            concept_ids: Vec::new(),
            entity_ids: Vec::new(),
            since: None,
            until: None,
            code_index_keys: None,
            min_confidence: None,
            include_archived: None,
            as_of: None,
            include_code_artifacts: include,
        }
    }

    #[test]
    fn suppression_defaults_on_and_opts_in_explicitly() {
        assert!(suppresses_code_artifacts(&request(None)));
        assert!(suppresses_code_artifacts(&request(Some(filter(None)))));
        assert!(suppresses_code_artifacts(&request(Some(filter(Some(
            false
        ))))));
        assert!(!suppresses_code_artifacts(&request(Some(filter(Some(
            true
        ))))));
    }
}
