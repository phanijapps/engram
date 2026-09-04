//! Dead-code liveness classification (spec: code-liveness T4/T5).
//!
//! Liveness inputs, in order of strength:
//! 1. Resolved call in-degree (`calls` family) — direct.
//! 2. Registration: the type is the target of a `registers` edge — the
//!    type is live AND its contained members are dispatch-reachable
//!    (registries and `Arc<dyn Trait>` leave no static call edges).
//! 3. Contains-liveness: a container whose member is live is itself live.
//!
//! Buckets (evidence-backed set; `generated` and `inconclusive` are
//! deferred — see `docs/backlog.md` `code-liveness-followups`):
//! - `confirmed-unused` — no liveness evidence, not test-shaped.
//! - `dynamic` — no direct in-degree, but contained by a live or
//!   registered container: reachable through dispatch/registry.
//! - `test-only` — test-shaped name (`Failing*`/`Fake*`/`Stub*`/`Mock*`)
//!   or contained under a `tests` module.
//! - `external` — entry-point-shaped (`main`): zero in-degree by design.

use std::collections::HashSet;

use crate::queries::AnalyticsGraph;

/// Evidence-backed dead-code buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeadCodeBucket {
    ConfirmedUnused,
    Dynamic,
    TestOnly,
    External,
}

impl DeadCodeBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            DeadCodeBucket::ConfirmedUnused => "confirmed-unused",
            DeadCodeBucket::Dynamic => "dynamic",
            DeadCodeBucket::TestOnly => "test-only",
            DeadCodeBucket::External => "external",
        }
    }
}

/// One classified dead-code candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedSymbol {
    pub name: String,
    pub bucket: DeadCodeBucket,
}

/// The classified report — every raw dead-code candidate lands in exactly
/// one bucket; buckets are disjoint and sorted for determinism.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadCodeReport {
    pub confirmed_unused: Vec<String>,
    pub dynamic: Vec<String>,
    pub test_only: Vec<String>,
    pub external: Vec<String>,
}

impl DeadCodeReport {
    /// Flat (name, bucket) view, name-sorted — the transport shape.
    pub fn flat(&self) -> Vec<ClassifiedSymbol> {
        let mut out: Vec<ClassifiedSymbol> = self
            .confirmed_unused
            .iter()
            .map(|n| (n, DeadCodeBucket::ConfirmedUnused))
            .chain(self.dynamic.iter().map(|n| (n, DeadCodeBucket::Dynamic)))
            .chain(self.test_only.iter().map(|n| (n, DeadCodeBucket::TestOnly)))
            .chain(self.external.iter().map(|n| (n, DeadCodeBucket::External)))
            .map(|(name, bucket)| ClassifiedSymbol {
                name: name.clone(),
                bucket,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// Classifies every raw dead-code candidate of the graph.
pub fn classify_dead(graph: &AnalyticsGraph) -> DeadCodeReport {
    let live_map = engram_graph_analytics::in_degree(graph.edges());
    let live: HashSet<&str> = live_map.keys().map(|k| k.as_str()).collect();
    let registered: HashSet<&str> = graph
        .registers_edges()
        .iter()
        .map(|(_, ty)| ty.as_str())
        .collect();
    // Live-or-registered containers keep their contained members out of the
    // confirmed bucket — dispatch/registry reachable.
    let live_containers: HashSet<String> = registered
        .iter()
        .map(|s| s.to_string())
        .chain(
            live.iter()
                .filter(|id| {
                    graph
                        .contains_edges()
                        .iter()
                        .any(|(parent, _)| parent.as_str() == **id)
                })
                .map(|s| s.to_string()),
        )
        .collect();

    let mut report = DeadCodeReport::default();
    for candidate in raw_candidates(graph) {
        let name = graph.display(candidate).to_owned();
        if is_test_shaped(&name)
            || is_tests_container(&name)
            || contained_under_tests(graph, candidate)
        {
            report.test_only.push(name);
        } else if name == "main" {
            report.external.push(name);
        } else if contained_in_live_container(graph, candidate, &live_containers) {
            report.dynamic.push(name);
        } else {
            report.confirmed_unused.push(name);
        }
    }
    report.confirmed_unused.sort();
    report.dynamic.sort();
    report.test_only.sort();
    report.external.sort();
    report
}

/// Raw candidates: nodes present in the graph with no liveness evidence —
/// zero call in-degree, no contains-liveness, not registered. The
/// classifier buckets these; `dead_code` reports them as the raw list.
pub(crate) fn raw_candidates(graph: &AnalyticsGraph) -> Vec<&str> {
    let live_map = engram_graph_analytics::in_degree(graph.edges());
    let live: HashSet<&str> = live_map.keys().map(|k| k.as_str()).collect();
    let registered: HashSet<&str> = graph
        .registers_edges()
        .iter()
        .map(|(_, ty)| ty.as_str())
        .collect();
    let mut defined: HashSet<&str> = HashSet::new();
    for (a, b) in graph.edges() {
        defined.insert(a);
        defined.insert(b);
    }
    for (a, b) in graph.contains_edges() {
        defined.insert(a);
        defined.insert(b);
    }
    for (a, b) in graph.registers_edges() {
        defined.insert(a);
        defined.insert(b);
    }
    let mut out: Vec<&str> = defined
        .iter()
        .filter(|id| {
            !live.contains(*id)
                && !registered.contains(*id)
                && !graph
                    .contains_edges()
                    .iter()
                    .any(|(parent, child)| parent == *id && live.contains(child.as_str()))
        })
        .copied()
        .collect();
    out.sort_unstable();
    out
}

fn contained_in_live_container(
    graph: &AnalyticsGraph,
    id: &str,
    live_containers: &HashSet<String>,
) -> bool {
    graph
        .contains_edges()
        .iter()
        .any(|(parent, child)| child == id && live_containers.contains(parent))
}

fn contained_under_tests(graph: &AnalyticsGraph, id: &str) -> bool {
    graph
        .contains_edges()
        .iter()
        .any(|(parent, child)| child == id && is_tests_container(graph.display(parent)))
}

fn is_tests_container(name: &str) -> bool {
    name == "tests" || name.ends_with("::tests")
}

fn is_test_shaped(name: &str) -> bool {
    ["Failing", "Fake", "Stub", "Mock"]
        .iter()
        .any(|p| name.starts_with(p) || name.contains(&format!("::{p}")))
}

/// `dead_code`'s raw list over the full liveness inputs, display-named.
pub(crate) fn raw_dead_code(graph: &AnalyticsGraph) -> Vec<String> {
    graph.display_all(raw_candidates(graph))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::AnalyticsGraph;
    use engram_domain::{EntityRef, KnowledgeRelationship};

    fn rel(predicate: &str, subject: &str, object: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: engram_domain::types::RelationshipId::from(format!(
                "{predicate}:{subject}:{object}"
            )),
            graph_id: None,
            subject: EntityRef {
                id: Some(engram_domain::types::EntityId::from(subject)),
                kind: None,
                name: Some(subject.to_owned()),
                aliases: Vec::new(),
            },
            predicate: predicate.to_owned(),
            object: EntityRef {
                id: Some(engram_domain::types::EntityId::from(object)),
                kind: None,
                name: Some(object.to_owned()),
                aliases: Vec::new(),
            },
            scope: test_scope(),
            evidence: Vec::new(),
            confidence: None,
            provenance: test_provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
        }
    }

    fn test_scope() -> engram_domain::Scope {
        engram_domain::Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn test_provenance() -> engram_domain::Provenance {
        engram_domain::Provenance {
            source: "test".to_owned(),
            actor: engram_domain::Actor {
                id: engram_domain::types::Id::from("a"),
                kind: engram_domain::ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            method: None,
            confidence: None,
            derivations: Vec::new(),
            evidence: Vec::new(),
        }
    }

    /// code-liveness T4: a registered type's impl methods classify
    /// `dynamic`, never `confirmed-unused` — the `MemoryLifecycleExecutor`
    /// shape (registered behind `Arc<dyn ConsolidationMutationExecutor>`,
    /// `execute` reached only through the vtable).
    #[test]
    fn registered_type_impl_methods_are_dynamic() {
        let rels = vec![
            rel("calls", "main", "bootstrap"),
            rel("registers", "bootstrap", "MemoryLifecycleExecutor"),
            rel(
                "contains",
                "MemoryLifecycleExecutor",
                "MemoryLifecycleExecutor::execute",
            ),
        ];
        let graph = AnalyticsGraph::from_relationships(&rels);
        let report = classify_dead(&graph);
        assert!(
            report.confirmed_unused.is_empty(),
            "registered type and its methods never confirmed-unused: {report:?}"
        );
        assert!(
            report
                .dynamic
                .contains(&"MemoryLifecycleExecutor::execute".to_owned())
        );
        // The registered TYPE is live — absent from every bucket. Its
        // dispatch-only impl method is `dynamic`, never confirmed-unused.
        let flat = report.flat();
        assert!(!flat.iter().any(|s| s.name == "MemoryLifecycleExecutor"));
        assert!(!report.confirmed_unused.contains(&"bootstrap".to_owned()));
    }

    #[test]
    fn test_doubles_classify_test_only() {
        let rels = vec![
            rel("contains", "tests", "FailingMemory"),
            rel("contains", "FailingMemory", "FailingMemory::get"),
        ];
        let graph = AnalyticsGraph::from_relationships(&rels);
        let report = classify_dead(&graph);
        assert!(report.test_only.contains(&"FailingMemory".to_owned()));
        assert!(report.test_only.contains(&"FailingMemory::get".to_owned()));
        assert!(report.confirmed_unused.is_empty());
    }

    #[test]
    fn uncalled_isolated_symbol_is_confirmed_unused() {
        let rels = vec![rel("calls", "a", "b"), rel("calls", "b", "c")];
        let graph = AnalyticsGraph::from_relationships(&rels);
        let report = classify_dead(&graph);
        assert_eq!(report.confirmed_unused, vec!["a".to_owned()]);
    }

    #[test]
    fn main_classifies_external() {
        let rels = vec![rel("contains", "app", "main")];
        let graph = AnalyticsGraph::from_relationships(&rels);
        let report = classify_dead(&graph);
        assert!(report.external.contains(&"main".to_owned()));
    }
}
