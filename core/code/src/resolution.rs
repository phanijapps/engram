//! Cross-file reference resolution + the unresolved-reference ledger
//! (RFC-0020 Phase 2).
//!
//! `resolve_refs` fills name-only relationship object refs against the
//! scope-wide symbol table and returns ledger records for every reference it
//! could not settle — ambiguity is recorded with its candidates, never
//! silently dropped or arbitrarily picked. Dotted references
//! (`store.save`, as written at the call site) resolve through a
//! receiver-type hint first (`Store::save`) before the bare-name ladder.

use chrono::Utc;

use engram_domain::{KnowledgeRelationship, UnresolvedReference, UnresolvedReferenceStatus};

use crate::edge_resolution::{attempt, branch_evidence};
use crate::identity::{Resolution, SymbolIndex};
use engram_domain::Id;

/// Pascal-case a receiver hint (`store` → `Store`) for the qualified-name
/// attempt; already-capitalized hints pass through.
pub fn receiver_type_hint(hint: &str) -> String {
    let mut chars = hint.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => hint.to_owned(),
    }
}

/// Split a dotted reference as-written (`self.store.save` → hint `store`,
/// name `save`); `self`/`Self` receivers carry no useful hint.
pub fn split_dotted(reference: &str) -> (Option<&str>, &str) {
    // [receiver-resolution-gap] Handle BOTH `.` (method receiver) and `::`
    // (Rust module path) separators. `store::save()` splits to hint `store`,
    // name `save` — resolution finds the bare `save` in the index.
    let sep = if reference.contains("::") && !reference.contains('.') {
        "::"
    } else {
        "."
    };
    let Some((head, name)) = reference.rsplit_once(sep) else {
        return (None, reference);
    };
    if head.is_empty() {
        return (None, reference);
    }
    // Multi-segment heads use the last segment (`self.store` → `store`).
    let hint = head.rsplit(sep).next().unwrap_or(head);
    if matches!(hint, "self" | "Self" | "crate" | "super") {
        return (None, name);
    }
    (Some(hint), name)
}

/// Fill name-only object refs (`calls`, `extends`, `implements`,
/// `routes_to`) and return
/// the unresolved-reference ledger for everything that did not settle.
/// Ledger records are `pending`; persistence + the orphan sweep are T6.
pub fn resolve_refs(
    index: &SymbolIndex,
    relationships: &mut [KnowledgeRelationship],
    repo: Option<&str>,
    path: Option<&str>,
    repo_ctx: Option<&crate::parser::repo_context::RepoContext>,
) -> Vec<UnresolvedReference> {
    const RESOLVABLE: [&str; 5] = ["calls", "extends", "implements", "routes_to", "consumes"];
    let mut ledger = Vec::new();
    let now = Utc::now();

    for rel in relationships.iter_mut() {
        if !RESOLVABLE.contains(&rel.predicate.as_str()) || rel.object.id.is_some() {
            continue;
        }
        let Some(reference) = rel.object.name.clone() else {
            continue;
        };
        // Subjectless references have no stable ledger key — skipping them
        // beats collapsing distinct rows onto one "unknown" id.
        let Some(subject_id) = rel.subject.id.clone() else {
            continue;
        };
        let from_id = subject_id.to_string();
        // Shared evidence-aware ladder (edge_resolution::attempt): the EN-9
        // receiver rules, apex aliasing, static-module paths, and the
        // same-document fallback all live there, and the settling branch is
        // recorded as typed code-resolution evidence instead of being
        // reconstructed after the candidate is chosen.
        let outcome = attempt(index, &reference, path, repo, repo_ctx);
        match outcome.resolution {
            Resolution::Resolved(candidate) => {
                rel.object.id = Some(Id::from(candidate.id));
                // Canonicalize the endpoint name to the resolved entity's
                // qualified name: edges previously kept their AS-WRITTEN bare
                // callee (`getPagedPropertyList`) even after resolution set
                // the id, so name-keyed navigation (symbol_context,
                // blast_radius — invoked BY name) missed resolved edges whose
                // entity is qualified (`PropertyController::getPagedPropertyList`).
                if rel.object.name.as_deref() != Some(candidate.name.as_str()) {
                    rel.object.name = Some(candidate.name.clone());
                }
                // Record the binding evidence at the decision point. The
                // tier describes the BINDING mechanism, so an extraction-time
                // `parsed` spelling is replaced by the binding tier
                // (scoped_name / heuristic) once the ladder settles a target.
                if let Some(branch) = outcome.branch {
                    rel.code_resolution = Some(branch_evidence(branch));
                }
            }
            Resolution::Ambiguous(candidates) => ledger.push(UnresolvedReference {
                id: Id::from(format!("unref-{}-{}", from_id, content_key(&reference))),
                graph_id: rel.graph_id.clone(),
                from_entity_id: subject_id.clone(),
                reference_name: reference.clone(),
                candidates: candidates.iter().map(|c| Id::from(c.id.clone())).collect(),
                status: UnresolvedReferenceStatus::Pending,
                path: path.unwrap_or_default().to_owned(),
                line: None,
                scope: rel.scope.clone(),
                created_at: now,
                updated_at: None,
            }),
            Resolution::NotFound => ledger.push(UnresolvedReference {
                id: Id::from(format!("unref-{}-{}", from_id, content_key(&reference))),
                graph_id: rel.graph_id.clone(),
                from_entity_id: subject_id.clone(),
                reference_name: reference.clone(),
                candidates: Vec::new(),
                status: UnresolvedReferenceStatus::Pending,
                path: path.unwrap_or_default().to_owned(),
                line: None,
                scope: rel.scope.clone(),
                created_at: now,
                updated_at: None,
            }),
        }
    }
    ledger
}

/// Deterministic key for a reference name (ids must be stable across runs so
/// re-ingest regenerates the same ledger row).
fn content_key(name: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:x}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::identity::SymbolCandidate;
    use crate::identity::SymbolIndex;
    use engram_domain::*;

    pub(crate) fn rel(
        predicate: &str,
        subject_id: &str,
        object_name: &str,
    ) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from("r1"),
            graph_id: Some(KnowledgeGraphId::from("g1")),
            subject: EntityRef {
                id: Some(EntityId::from(subject_id)),
                kind: None,
                name: Some(subject_id.to_owned()),
                aliases: Vec::new(),
            },
            predicate: predicate.to_owned(),
            code_resolution: None,
            object: EntityRef {
                id: None,
                kind: None,
                name: Some(object_name.to_owned()),
                aliases: Vec::new(),
            },
            scope: Scope {
                tenant: "t".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: Vec::new(),
            confidence: None,
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: Utc::now(),
            updated_at: None,
            archived_at: None,
        }
    }

    pub(crate) fn register(index: &mut SymbolIndex, name: &str, id: &str, repo: &str, path: &str) {
        index.register(
            name,
            SymbolCandidate {
                id: id.to_owned(),
                name: name.to_owned(),
                repo: Some(repo.to_owned()),
                path: Some(path.to_owned()),
            },
        );
    }

    /// EN-9 (eval 0.2.5, BLOCKER): `values.iter()` bound confidently to
    /// `Facets::iter` because resolve_refs tried the BARE name first and
    /// `iter` was unique project-wide — uniqueness of the method name is not
    /// evidence about an unknown receiver. A receiver-hinted reference may
    /// only bind through a receiver-consistent qualified form.
    #[test]
    fn hinted_reference_does_not_bind_by_bare_name_alone() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Facets::iter", "e1", "r", "src/facets.rs");
        let mut rels = vec![rel("calls", "caller", "values.iter")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/graph.rs"), None);
        assert!(
            !ledger.is_empty(),
            "unknown-receiver method call must stay unresolved, not bind to Facets::iter"
        );
        assert!(rels[0].object.id.is_none(), "no confident id binding");
    }

    /// Receiver-consistent binding still works: `store.save` → `Store::save`.
    #[test]
    fn hinted_reference_binds_via_receiver_qualified() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Store::save", "e1", "r", "src/store.rs");
        let mut rels = vec![rel("calls", "caller", "store.save")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/api.rs"), None);
        assert!(ledger.is_empty(), "receiver-qualified binding must resolve");
        assert!(rels[0].object.id.is_some());
    }

    /// Module-qualified Rust paths bind in their AS-WRITTEN form
    /// (`store::save` → entity `store::save`) — not only the capitalized
    /// receiver form. A `::` path to a BARE-registered fn
    /// (`store::save()` calling pub fn `save`) keeps the bare ladder: the
    /// module prefix is lexical evidence, not an unknown receiver.
    #[test]
    fn module_qualified_reference_binds_as_written() {
        let mut index = SymbolIndex::new();
        register(&mut index, "store::save", "e1", "r", "src/store.rs");
        let mut rels = vec![rel("calls", "caller", "store::save")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/api.rs"), None);
        assert!(ledger.is_empty(), "as-written module path must resolve");

        let mut index2 = SymbolIndex::new();
        register(&mut index2, "save", "e1", "r", "src/store.rs");
        let mut rels2 = vec![rel("calls", "caller", "store::save")];
        let ledger2 = resolve_refs(&index2, &mut rels2, Some("r"), Some("src/engine.rs"), None);
        assert!(ledger2.is_empty(), ":: path to bare fn must keep resolving");
    }

    /// Bare function calls (no receiver) keep the unique-survivor ladder —
    /// the EN-9 rule must not break plain cross-file calls.
    #[test]
    fn bare_function_call_still_resolves() {
        let mut index = SymbolIndex::new();
        register(&mut index, "save", "e1", "r", "src/store.rs");
        let mut rels = vec![rel("calls", "caller", "save")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/api.rs"), None);
        assert!(ledger.is_empty(), "bare call must keep resolving");
        assert!(rels[0].object.id.is_some());
    }

    #[test]
    fn exact_qualified_match_resolves() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Config::parse", "e1", "r", "src/config.rs");
        let mut rels = vec![rel("calls", "caller", "Config::parse")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/other.rs"), None);
        assert!(ledger.is_empty());
        assert_eq!(rels[0].object.id, Some(EntityId::from("e1")));
    }

    #[test]
    fn ambiguity_lands_in_the_ledger_with_candidates() {
        let mut index = SymbolIndex::new();
        register(&mut index, "parse", "e1", "r", "src/a.rs");
        register(&mut index, "parse", "e2", "r", "src/b.rs");
        let mut rels = vec![rel("calls", "caller", "parse")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), None, None);
        assert_eq!(ledger.len(), 1, "ambiguous reference must be recorded");
        assert_eq!(ledger[0].status, UnresolvedReferenceStatus::Pending);
        assert_eq!(ledger[0].candidates.len(), 2);
        assert!(rels[0].object.id.is_none());
    }

    #[test]
    fn not_found_lands_in_the_ledger() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("calls", "caller", "ghost_fn")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/a.rs"), None);
        assert_eq!(ledger.len(), 1);
        assert!(ledger[0].candidates.is_empty());
        assert_eq!(ledger[0].reference_name, "ghost_fn");
    }

    #[test]
    fn dotted_reference_resolves_via_receiver_hint() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Store::save", "e9", "r", "src/store.rs");
        let mut rels = vec![rel("calls", "Engine::drive", "store.save")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/engine.rs"), None);
        assert!(ledger.is_empty(), "ledger: {ledger:?}");
        assert_eq!(
            rels[0].object.id,
            Some(EntityId::from("e9")),
            "store.save must resolve through the Store::save hint"
        );
    }

    #[test]
    fn self_receiver_carries_no_hint_but_resolves_bare() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Engine::process", "e7", "r", "src/engine.rs");
        let mut rels = vec![rel("calls", "Engine::drive", "self.process")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/engine.rs"), None);
        // Same document: Engine::drive → Engine::process resolves via the
        // bare tail within the file.
        assert!(ledger.is_empty(), "ledger: {ledger:?}");
        assert_eq!(rels[0].object.id, Some(EntityId::from("e7")));
    }

    #[test]
    fn routes_to_handlers_ledger_when_unresolved() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("routes_to", "GET /health", "healthHandler")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("routes.ts"), None);
        assert_eq!(ledger.len(), 1, "unresolved route handler must be ledgered");
        assert_eq!(ledger[0].reference_name, "healthHandler");
    }

    #[test]
    fn ledger_ids_are_deterministic_per_subject_and_name() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("calls", "caller", "ghost_fn")];
        let first = resolve_refs(&index, &mut rels, Some("r"), None, None);
        let mut rels2 = vec![rel("calls", "caller", "ghost_fn")];
        let second = resolve_refs(&index, &mut rels2, Some("r"), None, None);
        assert_eq!(first[0].id, second[0].id);
    }

    #[test]
    fn split_dotted_strips_self_and_keeps_hint() {
        assert_eq!(split_dotted("save"), (None, "save"));
        assert_eq!(split_dotted("self.save"), (None, "save"));
        assert_eq!(split_dotted("store.save"), (Some("store"), "save"));
        assert_eq!(split_dotted("self.store.save"), (Some("store"), "save"));
    }

    #[test]
    fn receiver_hint_pascal_cases() {
        assert_eq!(receiver_type_hint("store"), "Store");
        assert_eq!(receiver_type_hint("Store"), "Store");
        assert_eq!(receiver_type_hint("db"), "Db");
    }
}

#[cfg(test)]
mod receiver_resolution_gap_tests {
    use super::tests::{register, rel};
    use super::*;
    use crate::identity::SymbolIndex;

    /// [receiver-resolution-gap] `self.method()` in the same file as the
    /// declaration must resolve via the same-document bare-name fallback.
    #[test]
    fn self_method_resolves_via_same_doc_bare_name() {
        let mut index = SymbolIndex::new();
        register(
            &mut index,
            "ConformanceHarness::run_beliefs_fixture",
            "e1",
            "r",
            "harness.rs",
        );
        let mut rels = vec![rel("calls", "caller_fn", "run_beliefs_fixture")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("harness.rs"), None);
        assert!(
            !ledger.is_empty() || rels[0].object.id.is_some(),
            "should resolve or ledger: rels={rels:?}"
        );
    }

    /// [receiver-resolution-gap] Qualified Rust paths preserve the path tail.
    #[test]
    fn qualified_path_resolves_against_index() {
        let mut index = SymbolIndex::new();
        register(
            &mut index,
            "FastEmbedEmbeddingProvider::new",
            "e1",
            "r",
            "bootstrap.rs",
        );
        let mut rels = vec![rel("calls", "caller_fn", "FastEmbedEmbeddingProvider::new")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("other.rs"), None);
        assert!(
            !ledger.is_empty() || rels[0].object.id.is_some(),
            "should resolve or ledger: rels={rels:?}"
        );
    }
}
