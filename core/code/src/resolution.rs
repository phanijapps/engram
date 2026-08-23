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

use crate::identity::{Resolution, SymbolCandidate, SymbolIndex};
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
    let Some((head, name)) = reference.rsplit_once('.') else {
        return (None, reference);
    };
    if head.is_empty() {
        return (None, reference);
    }
    // Multi-segment heads use the last segment (`self.store` → `store`).
    let hint = head.rsplit('.').next().unwrap_or(head);
    if matches!(hint, "self" | "Self" | "crate" | "super") {
        return (None, name);
    }
    (Some(hint), name)
}

/// Resolution ladder for one reference name from (optional) file context.
/// Returns the settled candidate or the surviving ambiguity.
fn resolve_one(
    index: &SymbolIndex,
    name: &str,
    from_path: Option<&str>,
    from_repo: Option<&str>,
) -> Resolution {
    // Same document first, then the SymbolIndex ladder (same repo / unique
    // survivor / ambiguity). The planned import-scope rung was removed: raw
    // import strings (`./x`, `crate::x`, `.x`) never suffix-matched real
    // file paths except as cross-directory false positives — recorded in
    // the spec notes + plan changelog.
    if let Some(path) = from_path {
        if let Resolution::Resolved(c) = index.resolve(name, Some(path), from_repo) {
            return Resolution::Resolved(c);
        }
    }
    index.resolve(name, from_path, from_repo)
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
) -> Vec<UnresolvedReference> {
    const RESOLVABLE: [&str; 4] = ["calls", "extends", "implements", "routes_to"];
    let mut ledger = Vec::new();
    let now = Utc::now();

    for rel in relationships.iter_mut() {
        if !RESOLVABLE.contains(&rel.predicate.as_str()) || rel.object.id.is_some() {
            continue;
        }
        let Some(reference) = rel.object.name.clone() else {
            continue;
        };
        let (hint, name) = split_dotted(&reference);
        // Subjectless references have no stable ledger key — skipping them
        // beats collapsing distinct rows onto one "unknown" id.
        let Some(subject_id) = rel.subject.id.clone() else {
            continue;
        };
        let from_id = subject_id.to_string();
        let mut outcome = resolve_one(index, name, path, repo);

        // Receiver-hint attempt: `store.save` → `Store::save` when a
        // declaration with that receiver exists.
        if !matches!(outcome, Resolution::Resolved(_)) {
            if let Some(hint) = hint {
                let qualified_attempt = format!("{}::{}", receiver_type_hint(hint), name);
                outcome = resolve_one(index, &qualified_attempt, path, repo);
            }
        }

        match outcome {
            Resolution::Resolved(candidate) => {
                rel.object.id = Some(Id::from(candidate.id));
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
mod tests {
    use super::*;
    use crate::identity::SymbolIndex;
    use engram_domain::*;

    fn rel(predicate: &str, subject_id: &str, object_name: &str) -> KnowledgeRelationship {
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

    fn register(index: &mut SymbolIndex, name: &str, id: &str, repo: &str, path: &str) {
        index.register(
            name,
            SymbolCandidate {
                id: id.to_owned(),
                repo: Some(repo.to_owned()),
                path: Some(path.to_owned()),
            },
        );
    }

    #[test]
    fn exact_qualified_match_resolves() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Config::parse", "e1", "r", "src/config.rs");
        let mut rels = vec![rel("calls", "caller", "Config::parse")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/other.rs"));
        assert!(ledger.is_empty());
        assert_eq!(rels[0].object.id, Some(EntityId::from("e1")));
    }

    #[test]
    fn ambiguity_lands_in_the_ledger_with_candidates() {
        let mut index = SymbolIndex::new();
        register(&mut index, "parse", "e1", "r", "src/a.rs");
        register(&mut index, "parse", "e2", "r", "src/b.rs");
        let mut rels = vec![rel("calls", "caller", "parse")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), None);
        assert_eq!(ledger.len(), 1, "ambiguous reference must be recorded");
        assert_eq!(ledger[0].status, UnresolvedReferenceStatus::Pending);
        assert_eq!(ledger[0].candidates.len(), 2);
        assert!(rels[0].object.id.is_none());
    }

    #[test]
    fn not_found_lands_in_the_ledger() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("calls", "caller", "ghost_fn")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/a.rs"));
        assert_eq!(ledger.len(), 1);
        assert!(ledger[0].candidates.is_empty());
        assert_eq!(ledger[0].reference_name, "ghost_fn");
    }

    #[test]
    fn dotted_reference_resolves_via_receiver_hint() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Store::save", "e9", "r", "src/store.rs");
        let mut rels = vec![rel("calls", "Engine::drive", "store.save")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/engine.rs"));
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
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("src/engine.rs"));
        // Same document: Engine::drive → Engine::process resolves via the
        // bare tail within the file.
        assert!(ledger.is_empty(), "ledger: {ledger:?}");
        assert_eq!(rels[0].object.id, Some(EntityId::from("e7")));
    }

    #[test]
    fn routes_to_handlers_ledger_when_unresolved() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("routes_to", "GET /health", "healthHandler")];
        let ledger = resolve_refs(&index, &mut rels, Some("r"), Some("routes.ts"));
        assert_eq!(ledger.len(), 1, "unresolved route handler must be ledgered");
        assert_eq!(ledger[0].reference_name, "healthHandler");
    }

    #[test]
    fn ledger_ids_are_deterministic_per_subject_and_name() {
        let index = SymbolIndex::new();
        let mut rels = vec![rel("calls", "caller", "ghost_fn")];
        let first = resolve_refs(&index, &mut rels, Some("r"), None);
        let mut rels2 = vec![rel("calls", "caller", "ghost_fn")];
        let second = resolve_refs(&index, &mut rels2, Some("r"), None);
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
