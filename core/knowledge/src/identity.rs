//! Entity identity and consolidation — storage-neutral identity operations
//! for knowledge-graph entities and relationships (RFC-0014).
//!
//! The port trait (`EntityIdentityRepository`) defines the contract; adapters
//! (SQLite, SurrealDB) implement it behind engine-private indexes/tables.
//! The pure functions (`normalize_name`, `compute_identity_key`,
//! `compute_relationship_key`, `merge_entities`) are shared across all adapters
//! and tested here, not in adapter crates.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

// ── Port trait ───────────────────────────────────────────────────────────────

/// Identity-aware entity + relationship operations, plus transactional
/// consolidation. Storage-neutral; SQLite indexes, SurrealDB unique constraints,
/// and transaction strategy are adapter-private.
#[async_trait]
pub trait EntityIdentityRepository: Send + Sync {
    /// Atomically resolve an entity under a declared identity policy.
    async fn resolve_or_put_entity(
        &self,
        request: EntityWriteRequest,
    ) -> CoreResult<EntityWriteOutcome>;

    /// Atomically resolve a relationship by its exact canonical key
    /// (scope + graph + subject + object + predicate).
    async fn resolve_or_put_relationship(
        &self,
        relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship>;

    /// Dry-run: discover entities that collide under a declared identity policy.
    async fn discover_collisions(
        &self,
        scope: &Scope,
        mode: &EntityIdentityMode,
    ) -> CoreResult<Vec<CollisionGroup>>;

    /// Transactionally consolidate duplicate entity IDs into a canonical entity.
    async fn consolidate_entities(
        &self,
        request: EntityMergeRequest,
    ) -> CoreResult<EntityMergeResult>;
}

// ── Pure normalization and merge functions (shared across adapters) ──────────

/// Normalize a name for identity comparison: trim, collapse whitespace,
/// lowercase.
pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compute the identity key for an entity under a declared mode, or `None`
/// if the mode is `IdOnly` (no identity resolution).
pub fn compute_identity_key(
    entity: &KnowledgeEntity,
    mode: &EntityIdentityMode,
) -> Option<String> {
    match mode {
        EntityIdentityMode::IdOnly => None,
        EntityIdentityMode::StableKey { key } => Some(format!("sk:{}", key)),
        EntityIdentityMode::ScopedKindAndNormalizedName {
            include_graph,
            match_aliases: _,
            ..
        } => {
            let scope_part = &entity.scope.tenant;
            let kind_part = format!("{:?}", entity.kind);
            let name_part = normalize_name(&entity.name);
            let graph_part = if *include_graph {
                entity.graph_id.as_ref().map(|g| g.to_string()).unwrap_or_default()
            } else {
                String::new()
            };
            Some(format!("sn:{}|{}|{}|{}", scope_part, kind_part, name_part, graph_part))
        }
    }
}

/// Compute the exact canonical relationship key:
/// `scope_tenant|graph|subject_id|predicate|object_id`.
pub fn compute_relationship_key(rel: &KnowledgeRelationship) -> String {
    let subject = rel.subject.id.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let object = rel.object.id.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let graph = rel.graph_id.as_ref().map(|g| g.to_string()).unwrap_or_default();
    format!("{}|{}|{}|{}|{}", rel.scope.tenant, graph, subject, rel.predicate, object)
}

/// Merge a duplicate entity into a canonical entity.
///
/// - Union: `aliases`, `source_refs`, `concept_refs`, `ontology_class_refs`.
/// - Keep canonical: `kind`, `name`, `graph_id`, `scope`, `provenance`, `metadata`.
/// - Temporal: keep earliest `created_at`, latest `updated_at`.
/// - Report conflicts for scalar fields that differ.
pub fn merge_entities(
    canonical: &KnowledgeEntity,
    duplicate: &KnowledgeEntity,
    policy: &EntityMergePolicy,
) -> (KnowledgeEntity, Vec<String>, Vec<EntityMergeConflict>) {
    let mut merged = canonical.clone();
    let mut changed = Vec::new();
    let mut conflicts = Vec::new();

    // Union aliases (dedup by normalized form).
    for alias in &duplicate.aliases {
        let norm = normalize_name(alias);
        if !merged.aliases.iter().any(|a| normalize_name(a) == norm) {
            merged.aliases.push(alias.clone());
            changed.push("aliases".to_string());
        }
    }

    // Union source_refs (dedup by target_id).
    for sr in &duplicate.source_refs {
        if !merged.source_refs.iter().any(|e| e.target_id == sr.target_id) {
            merged.source_refs.push(sr.clone());
            changed.push("source_refs".to_string());
        }
    }

    // Union concept_refs (dedup by id).
    for cr in &duplicate.concept_refs {
        if !merged.concept_refs.iter().any(|c| c.id == cr.id) {
            merged.concept_refs.push(cr.clone());
            changed.push("concept_refs".to_string());
        }
    }

    // Union ontology_class_refs (dedup by value).
    for ocr in &duplicate.ontology_class_refs {
        if !merged.ontology_class_refs.contains(ocr) {
            merged.ontology_class_refs.push(ocr.clone());
            changed.push("ontology_class_refs".to_string());
        }
    }

    // Temporal: earliest created_at.
    if duplicate.created_at < merged.created_at {
        merged.created_at = duplicate.created_at;
        changed.push("created_at".to_string());
    }

    // Temporal: latest updated_at.
    match (merged.updated_at, duplicate.updated_at) {
        (Some(m), Some(d)) if d > m => {
            merged.updated_at = Some(d);
            changed.push("updated_at".to_string());
        }
        (None, Some(d)) => {
            merged.updated_at = Some(d);
            changed.push("updated_at".to_string());
        }
        _ => {}
    }

    // Report scalar conflicts.
    if canonical.kind != duplicate.kind {
        conflicts.push(EntityMergeConflict {
            field: "kind".to_string(),
            canonical_value: format!("{:?}", canonical.kind),
            duplicate_value: format!("{:?}", duplicate.kind),
        });
    }
    if normalize_name(&canonical.name) != normalize_name(&duplicate.name) {
        conflicts.push(EntityMergeConflict {
            field: "name".to_string(),
            canonical_value: canonical.name.clone(),
            duplicate_value: duplicate.name.clone(),
        });
    }

    // Apply conflict strategy.
    if !conflicts.is_empty() {
        match policy.conflict_strategy {
            ConflictStrategy::Report | ConflictStrategy::PreferCanonical => {}
            ConflictStrategy::PreferEarliest => {
                if duplicate.created_at < canonical.created_at {
                    merged.kind = duplicate.kind.clone();
                    changed.push("kind".to_string());
                }
            }
        }
    }

    (merged, changed, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> Timestamp {
        chrono::DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn actor() -> Actor {
        Actor {
            id: engram_domain::ActorId::from("test-actor"),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    fn test_entity(id: &str, name: &str, tenant: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: EntityId::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: name.to_string(),
            aliases: Vec::new(),
            scope: Scope {
                tenant: tenant.to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: Provenance {
                source: "test".to_string(),
                actor: actor(),
                observed_at: ts(0),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("manual".to_string()),
            },
            created_at: ts(100),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        }
    }

    fn test_relationship(id: &str, subject_id: &str, predicate: &str, object_id: &str, tenant: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from(id),
            graph_id: None,
            subject: EntityRef { id: Some(EntityId::from(subject_id)), kind: None, name: None, aliases: Vec::new() },
            predicate: predicate.to_string(),
            object: EntityRef { id: Some(EntityId::from(object_id)), kind: None, name: None, aliases: Vec::new() },
            scope: Scope { tenant: tenant.to_string(), subject: None, workspace: None, session: None, environment: None },
            evidence: Vec::new(),
            confidence: None,
            provenance: Provenance {
                source: "test".to_string(),
                actor: actor(),
                observed_at: ts(0),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("manual".to_string()),
            },
            created_at: ts(100),
            updated_at: None,
        }
    }

    // ── normalize_name ────────────────────────────────────────────────────

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_name("FastIndex"), "fastindex");
        assert_eq!(normalize_name("MCP Server"), "mcp server");
        assert_eq!(normalize_name("UBER"), "uber");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize_name("  hello   world  "), "hello world");
    }

    #[test]
    fn normalize_idempotent() {
        let once = normalize_name("Some  Mixed   CASE");
        assert_eq!(once, normalize_name(&once));
    }

    // ── compute_identity_key ──────────────────────────────────────────────

    #[test]
    fn identity_key_id_only_is_none() {
        let entity = test_entity("e1", "Foo", "tenant-a");
        assert!(compute_identity_key(&entity, &EntityIdentityMode::IdOnly).is_none());
    }

    #[test]
    fn identity_key_stable_key() {
        let entity = test_entity("e1", "Foo", "tenant-a");
        let key = compute_identity_key(&entity, &EntityIdentityMode::StableKey { key: "org-123".into() });
        assert_eq!(key.as_deref(), Some("sk:org-123"));
    }

    #[test]
    fn identity_key_case_variants_match() {
        let e1 = test_entity("e1", "FastIndex", "tenant-a");
        let e2 = test_entity("e2", "fastindex", "tenant-a");
        let mode = EntityIdentityMode::ScopedKindAndNormalizedName {
            normalization_version: "1".into(), include_graph: false, match_aliases: false,
        };
        assert_eq!(compute_identity_key(&e1, &mode), compute_identity_key(&e2, &mode));
    }

    #[test]
    fn identity_key_different_scopes_differ() {
        let e1 = test_entity("e1", "Foo", "tenant-a");
        let e2 = test_entity("e2", "Foo", "tenant-b");
        let mode = EntityIdentityMode::ScopedKindAndNormalizedName {
            normalization_version: "1".into(), include_graph: false, match_aliases: false,
        };
        assert_ne!(compute_identity_key(&e1, &mode), compute_identity_key(&e2, &mode));
    }

    // ── compute_relationship_key ──────────────────────────────────────────

    #[test]
    fn relationship_key_includes_all_parts() {
        let rel = test_relationship("r1", "subj-1", "uses", "obj-1", "tenant-a");
        let key = compute_relationship_key(&rel);
        assert!(key.contains("subj-1") && key.contains("uses") && key.contains("obj-1") && key.contains("tenant-a"));
    }

    #[test]
    fn relationship_key_different_predicates_differ() {
        let r1 = test_relationship("r1", "s", "uses", "o", "t");
        let r2 = test_relationship("r2", "s", "used_by", "o", "t");
        assert_ne!(compute_relationship_key(&r1), compute_relationship_key(&r2));
    }

    // ── merge_entities ────────────────────────────────────────────────────

    #[test]
    fn merge_unions_aliases() {
        let mut canonical = test_entity("e1", "Foo", "t");
        canonical.aliases = vec!["bar".into()];
        let mut duplicate = test_entity("e2", "foo", "t");
        duplicate.aliases = vec!["baz".into()];
        let (merged, changed, _) = merge_entities(&canonical, &duplicate, &EntityMergePolicy::default());
        assert!(merged.aliases.contains(&"bar".into()) && merged.aliases.contains(&"baz".into()));
        assert!(changed.contains(&"aliases".into()));
    }

    #[test]
    fn merge_keeps_earliest_created() {
        let mut canonical = test_entity("e1", "Foo", "t");
        canonical.created_at = ts(200);
        let mut duplicate = test_entity("e2", "foo", "t");
        duplicate.created_at = ts(100);
        let (merged, _, _) = merge_entities(&canonical, &duplicate, &EntityMergePolicy::default());
        assert_eq!(merged.created_at, ts(100));
    }

    #[test]
    fn merge_reports_kind_conflict() {
        let mut canonical = test_entity("e1", "Foo", "t");
        canonical.kind = EntityKind::Project;
        let mut duplicate = test_entity("e2", "foo", "t");
        duplicate.kind = EntityKind::Concept;
        let (_, _, conflicts) = merge_entities(&canonical, &duplicate, &EntityMergePolicy::default());
        assert!(conflicts.iter().any(|c| c.field == "kind"));
    }
}
