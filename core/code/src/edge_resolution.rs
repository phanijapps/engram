//! Code-edge resolution evidence (wicked-estate adoption, slice 1).
//!
//! Single home for two things both `resolve_refs` (core) and the orphan
//! sweep (adapter) need:
//!
//! 1. [`attempt`] — the pure reference-resolution ladder, returning not just
//!    the [`Resolution`] but *which branch* settled it, so the evidence is
//!    recorded at the decision point instead of reconstructed afterwards.
//! 2. Evidence construction: branch → typed [`CodeEdgeResolution`], stable
//!    producer ids, and the conservative legacy classifier for edges
//!    persisted before evidence existed.
//!
//! Tier semantics (see `engram_domain::ResolutionTier`): the tier describes
//! the *binding mechanism*. A name-only parsed edge keeps tier `parsed` with
//! `resolved=false` in query evidence — observing a call spelling in the AST
//! is not proof that an inferred destination entity is correct, and
//! receiver-capitalization inference can never acquire `parsed` merely
//! because the reference originated in AST text.

use engram_domain::{CodeEdgeKind, CodeEdgeResolution, KnowledgeRelationship, ResolutionTier};
use std::str::FromStr;

use crate::identity::{Resolution, SymbolIndex};
use crate::resolution::{receiver_type_hint, split_dotted};

/// Stable, versioned producer identifiers for code-edge evidence.
pub mod producer {
    /// Same-document AST call binding (object locally qualified at extraction).
    pub const AST_LOCAL: &str = "ast-local-v1";
    /// AST call spelling with a name-only object (binding pending the ladder).
    pub const AST_CALLS: &str = "ast-calls-v1";
    /// Symbol-index ladder: as-written/exact/same-doc/same-repo/unique rungs.
    pub const SCOPE_NAME: &str = "scope-name-v1";
    /// Receiver-capitalization inference (`store` → `Store::save`).
    pub const RECEIVER_CAPITALIZATION: &str = "receiver-capitalization-v1";
    /// Text co-occurrence fallback edges.
    pub const COOCCURRENCE: &str = "cooccurrence-v1";
    /// Framework-pattern synthesis (JSX callbacks, route wiring heuristics).
    pub const FRAMEWORK_PATTERN: &str = "framework-pattern-v1";
    /// Orphan-sweep healed edges (name-based binding, EN-9-consistent ladder).
    pub const ORPHAN_SWEEP: &str = "orphan-sweep-v1";
    /// Import statements (file → module; pure syntax).
    pub const PARSED_IMPORTS: &str = "parsed-imports-v1";
    /// Receiver-chain containment (pure structural fact).
    pub const PARSED_CONTAINS: &str = "parsed-contains-v1";
    /// `extends` / `implements` spellings (base may be external, name-only).
    pub const PARSED_HERITAGE: &str = "parsed-heritage-v1";
    /// Route decorator → handler spelling (handler binding pending the ladder).
    pub const PARSED_ROUTES: &str = "parsed-routes-v1";
}

/// Which ladder branch settled a reference attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionBranch {
    /// Deterministic name-table binding: as-written qualified form, apex
    /// aliasing, or the hintless/static-module bare ladder.
    NameTable,
    /// Receiver-capitalization inference — a guess about the receiver's type.
    ReceiverCapitalization,
}

/// Outcome of one reference-resolution attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptOutcome {
    pub resolution: Resolution,
    /// The branch that settled a `Resolved` outcome (`None` otherwise —
    /// ambiguity and not-found carry no binding evidence).
    pub branch: Option<ResolutionBranch>,
}

impl AttemptOutcome {
    /// Typed evidence for a resolved attempt; `None` while unbound.
    pub fn evidence(&self) -> Option<CodeEdgeResolution> {
        let branch = self.branch?;
        Some(branch_evidence(branch))
    }
}

/// Branch → typed evidence. Producer ids are stable and versioned; the tier
/// never overstates: capitalization inference stays heuristic.
pub fn branch_evidence(branch: ResolutionBranch) -> CodeEdgeResolution {
    match branch {
        ResolutionBranch::NameTable => {
            CodeEdgeResolution::new(ResolutionTier::ScopedName, producer::SCOPE_NAME)
        }
        ResolutionBranch::ReceiverCapitalization => {
            CodeEdgeResolution::new(ResolutionTier::Heuristic, producer::RECEIVER_CAPITALIZATION)
        }
    }
}

/// Evidence for the orphan sweep's healed edges (shares [`attempt`]; the
/// sweep passes no path context, so same-document branches cannot occur).
pub fn sweep_evidence(branch: ResolutionBranch) -> CodeEdgeResolution {
    CodeEdgeResolution::new(branch_tier(branch), producer::ORPHAN_SWEEP)
}

fn branch_tier(branch: ResolutionBranch) -> ResolutionTier {
    match branch {
        ResolutionBranch::NameTable => ResolutionTier::ScopedName,
        ResolutionBranch::ReceiverCapitalization => ResolutionTier::Heuristic,
    }
}

/// The pure reference-resolution ladder, shared by `resolve_refs` (core) and
/// the orphan sweep (adapter) so both stamp identical evidence. Behavior is
/// the RFC-0020 Phase-2 ladder: apex aliasing, same-document first, as-written
/// qualified form, receiver-capitalized form, static-module bare ladder, and
/// the same-document bare-name fallback. EN-9 holds everywhere: a
/// dot-hinted reference binds ONLY through receiver-consistent qualified
/// forms.
pub fn attempt(
    index: &SymbolIndex,
    reference: &str,
    from_path: Option<&str>,
    from_repo: Option<&str>,
) -> AttemptOutcome {
    let (hint, name) = split_dotted(reference);
    let static_module_path = hint.is_some() && reference.contains("::") && !reference.contains('.');

    // [js-apex-boundary] Salesforce Apex import aliasing — deterministic
    // name-table binding.
    if let Some(stripped) = reference.strip_prefix("@salesforce/apex/") {
        if let Some((class, method)) = stripped.rsplit_once('.') {
            let qualified = format!("{class}::{method}");
            if let Resolution::Resolved(c) = index.resolve(&qualified, from_path, from_repo) {
                return AttemptOutcome {
                    resolution: Resolution::Resolved(c),
                    branch: Some(ResolutionBranch::NameTable),
                };
            }
            if let Resolution::Resolved(c) = index.resolve(method, from_path, from_repo) {
                return AttemptOutcome {
                    resolution: Resolution::Resolved(c),
                    branch: Some(ResolutionBranch::NameTable),
                };
            }
        }
    }

    // Ladder proper. `allow_same_doc_fallback` mirrors the original
    // resolve_refs rule: hintless references and static module paths may use
    // the same-document bare fallback; dot-hinted method references may not
    // (EN-9: a same-document unique method name is still not receiver
    // evidence).
    // (The historic explicit same-document bare fallback is subsumed by the
    // plain ladder's same-document rung inside `SymbolIndex::resolve`.)
    let (outcome, _allow_same_doc_fallback) = match hint {
        None => (resolve_one(index, name, from_path, from_repo), true),
        Some(hint) => {
            // As-written qualified form first (deterministic) …
            let as_written = format!("{}::{}", hint, name);
            let mut o = resolve_one(index, &as_written, from_path, from_repo);
            if matches!(o, Resolution::Resolved(_)) {
                return AttemptOutcome {
                    resolution: o,
                    branch: Some(ResolutionBranch::NameTable),
                };
            }
            // … then the capitalized receiver guess (heuristic) …
            let capitalized = format!("{}::{}", receiver_type_hint(hint), name);
            o = resolve_one(index, &capitalized, from_path, from_repo);
            if matches!(o, Resolution::Resolved(_)) {
                return AttemptOutcome {
                    resolution: o,
                    branch: Some(ResolutionBranch::ReceiverCapitalization),
                };
            }
            // … and, for static module paths (`a::b` with `use a;`), the bare
            // ladder: the prefix is lexical evidence, not an unknown receiver.
            let allow_fallback = static_module_path;
            if static_module_path {
                o = resolve_one(index, name, from_path, from_repo);
                if matches!(o, Resolution::Resolved(_)) {
                    return AttemptOutcome {
                        resolution: o,
                        branch: Some(ResolutionBranch::NameTable),
                    };
                }
            }
            // Preserve the original ledger semantics: the surviving
            // ambiguity/not-found is whatever the LAST rung produced.
            (o, allow_fallback)
        }
    };

    // A settled plain-ladder outcome is a deterministic name-table binding.
    if let Resolution::Resolved(c) = &outcome {
        return AttemptOutcome {
            resolution: Resolution::Resolved(c.clone()),
            branch: Some(ResolutionBranch::NameTable),
        };
    }

    // Nothing settled through a known branch: return the best unbound
    // outcome (ambiguity/not-found) — no evidence.
    AttemptOutcome {
        resolution: outcome,
        branch: None,
    }
}

/// Same-document first, then the symbol-index ladder. (Moved from
/// `resolution.rs`; behavior unchanged.)
fn resolve_one(
    index: &SymbolIndex,
    name: &str,
    from_path: Option<&str>,
    from_repo: Option<&str>,
) -> Resolution {
    if let Some(path) = from_path {
        if let Resolution::Resolved(c) = index.resolve(name, Some(path), from_repo) {
            return Resolution::Resolved(c);
        }
    }
    index.resolve(name, from_path, from_repo)
}

/// Conservative effective evidence for a code relationship persisted before
/// evidence existed (`code_resolution` absent). Recognizes only proven
/// historical producer signals:
///
/// - `provenance.method == "deterministic_orphan_sweep"` → orphan-sweep heal
///   (name-based binding) → `scoped_name`.
/// - extraction (`deterministic_text_ingestion`) + `confidence == 0.5` → the
///   documented co-occurrence convention → `heuristic`.
/// - extraction + `imports` / `contains` → pure syntax facts (never
///   ladder-bound) → `parsed`.
/// - everything else (AST calls at 0.9 vs framework callbacks vs healed
///   routes — indistinguishable post-hoc) → `legacy_unknown`. A numeric
///   confidence alone never identifies a precise producer.
///
/// Non-code predicates (outside `CodeEdgeKind`) return `None`.
pub fn legacy_code_resolution(rel: &KnowledgeRelationship) -> Option<CodeEdgeResolution> {
    if let Some(existing) = &rel.code_resolution {
        return Some(existing.clone());
    }
    let predicate = rel.predicate.as_str();
    if CodeEdgeKind::from_str(predicate).is_err() {
        return None;
    }
    let method = rel.provenance.method.as_deref();
    match method {
        Some("deterministic_orphan_sweep") => Some(CodeEdgeResolution::new(
            ResolutionTier::ScopedName,
            producer::ORPHAN_SWEEP,
        )),
        Some("deterministic_text_ingestion") => match (predicate, rel.confidence) {
            (_, Some(0.5)) => Some(CodeEdgeResolution::new(
                ResolutionTier::Heuristic,
                producer::COOCCURRENCE,
            )),
            ("imports", _) => Some(CodeEdgeResolution::new(
                ResolutionTier::Parsed,
                producer::PARSED_IMPORTS,
            )),
            ("contains", _) => Some(CodeEdgeResolution::new(
                ResolutionTier::Parsed,
                producer::PARSED_CONTAINS,
            )),
            _ => Some(CodeEdgeResolution::new(
                ResolutionTier::LegacyUnknown,
                CodeEdgeResolution::LEGACY_PRODUCER,
            )),
        },
        _ => Some(CodeEdgeResolution::new(
            ResolutionTier::LegacyUnknown,
            CodeEdgeResolution::LEGACY_PRODUCER,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SymbolCandidate;
    use engram_domain::*;

    fn register(index: &mut SymbolIndex, name: &str, id: &str, repo: &str, path: &str) {
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

    fn resolved(out: &AttemptOutcome) -> bool {
        matches!(out.resolution, Resolution::Resolved(_))
    }

    #[test]
    fn as_written_qualified_bind_is_scoped_name() {
        let mut index = SymbolIndex::new();
        register(&mut index, "store::save", "e1", "r", "src/store.rs");
        let out = attempt(&index, "store::save", Some("src/api.rs"), Some("r"));
        assert!(resolved(&out));
        assert_eq!(out.branch, Some(ResolutionBranch::NameTable));
        let ev = out.evidence().unwrap();
        assert_eq!(ev.tier, ResolutionTier::ScopedName);
        assert_eq!(ev.resolved_by, producer::SCOPE_NAME);
    }

    #[test]
    fn capitalized_receiver_bind_is_heuristic() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Store::save", "e1", "r", "src/store.rs");
        let out = attempt(&index, "store.save", Some("src/api.rs"), Some("r"));
        assert!(resolved(&out));
        assert_eq!(out.branch, Some(ResolutionBranch::ReceiverCapitalization));
        let ev = out.evidence().unwrap();
        assert_eq!(ev.tier, ResolutionTier::Heuristic);
        assert_eq!(ev.resolved_by, producer::RECEIVER_CAPITALIZATION);
    }

    #[test]
    fn en9_dot_hinted_never_binds_by_bare_name() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Facets::iter", "e1", "r", "src/facets.rs");
        // `values.iter` registered under bare tail `iter` — unique survivor.
        let out = attempt(&index, "values.iter", Some("src/graph.rs"), Some("r"));
        assert!(
            !resolved(&out),
            "unknown-receiver method call must stay unresolved"
        );
        assert_eq!(out.branch, None);
        assert!(out.evidence().is_none());
    }

    #[test]
    fn same_document_bare_binding_is_scoped_name() {
        let mut index = SymbolIndex::new();
        register(&mut index, "Engine::process", "e7", "r", "src/engine.rs");
        let out = attempt(&index, "self.process", Some("src/engine.rs"), Some("r"));
        assert!(resolved(&out), "self.method in declaring file must bind");
        let ev = out.evidence().unwrap();
        assert_eq!(ev.tier, ResolutionTier::ScopedName);
        assert_eq!(ev.resolved_by, producer::SCOPE_NAME);
    }

    #[test]
    fn static_module_path_keeps_bare_ladder_as_name_table() {
        let mut index = SymbolIndex::new();
        register(&mut index, "save", "e1", "r", "src/store.rs");
        let out = attempt(&index, "store::save", Some("src/engine.rs"), Some("r"));
        assert!(resolved(&out));
        assert_eq!(out.branch, Some(ResolutionBranch::NameTable));
    }

    #[test]
    fn apex_alias_binds_as_name_table() {
        let mut index = SymbolIndex::new();
        register(
            &mut index,
            "PropertyController::getPaged",
            "e1",
            "r",
            "src/ctl.cls",
        );
        let out = attempt(
            &index,
            "@salesforce/apex/PropertyController.getPaged",
            Some("src/lwc.js"),
            Some("r"),
        );
        assert!(resolved(&out), "apex alias must resolve");
        assert_eq!(out.branch, Some(ResolutionBranch::NameTable));
    }

    #[test]
    fn sweep_evidence_uses_sweep_producer_with_branch_tier() {
        let heur = sweep_evidence(ResolutionBranch::ReceiverCapitalization);
        assert_eq!(heur.tier, ResolutionTier::Heuristic);
        assert_eq!(heur.resolved_by, producer::ORPHAN_SWEEP);
        let scoped = sweep_evidence(ResolutionBranch::NameTable);
        assert_eq!(scoped.tier, ResolutionTier::ScopedName);
        assert_eq!(scoped.resolved_by, producer::ORPHAN_SWEEP);
    }

    fn legacy_rel(
        predicate: &str,
        confidence: Option<f32>,
        method: Option<&str>,
    ) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from("r1"),
            graph_id: None,
            subject: EntityRef {
                id: Some(EntityId::from("s")),
                kind: None,
                name: Some("s".into()),
                aliases: vec![],
            },
            predicate: predicate.to_owned(),
            object: EntityRef {
                id: None,
                kind: None,
                name: Some("o".into()),
                aliases: vec![],
            },
            scope: Scope {
                tenant: "t".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: vec![],
            confidence,
            code_resolution: None,
            provenance: Provenance {
                source: "engram-ingest".into(),
                actor: Actor {
                    id: Id::from("a"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: vec![],
                derivations: vec![],
                confidence: None,
                method: method.map(str::to_owned),
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
        }
    }

    #[test]
    fn legacy_classifier_recognizes_only_proven_signals() {
        // Orphan sweep heal → scoped_name.
        let swept = legacy_rel("calls", Some(0.9), Some("deterministic_orphan_sweep"));
        let ev = legacy_code_resolution(&swept).unwrap();
        assert_eq!(ev.tier, ResolutionTier::ScopedName);
        assert_eq!(ev.resolved_by, producer::ORPHAN_SWEEP);

        // 0.5 ingestion → co-occurrence heuristic.
        let coocc = legacy_rel("calls", Some(0.5), Some("deterministic_text_ingestion"));
        let ev = legacy_code_resolution(&coocc).unwrap();
        assert_eq!(ev.tier, ResolutionTier::Heuristic);
        assert_eq!(ev.resolved_by, producer::COOCCURRENCE);

        // imports/contains are pure syntax → parsed.
        let imports = legacy_rel("imports", Some(1.0), Some("deterministic_text_ingestion"));
        assert_eq!(
            legacy_code_resolution(&imports).unwrap().tier,
            ResolutionTier::Parsed
        );
        let contains = legacy_rel("contains", Some(1.0), Some("deterministic_text_ingestion"));
        assert_eq!(
            legacy_code_resolution(&contains).unwrap().tier,
            ResolutionTier::Parsed
        );

        // AST calls (0.9) are producer-ambiguous → legacy_unknown.
        let calls = legacy_rel("calls", Some(0.9), Some("deterministic_text_ingestion"));
        let ev = legacy_code_resolution(&calls).unwrap();
        assert_eq!(ev.tier, ResolutionTier::LegacyUnknown);
        assert_eq!(ev.resolved_by, CodeEdgeResolution::LEGACY_PRODUCER);

        // High confidence alone never upgrades a tier.
        let loud = legacy_rel("extends", Some(1.0), Some("deterministic_text_ingestion"));
        assert_eq!(
            legacy_code_resolution(&loud).unwrap().tier,
            ResolutionTier::LegacyUnknown
        );
    }

    #[test]
    fn legacy_classifier_ignores_non_code_predicates_and_prefers_explicit() {
        let belongs = legacy_rel(
            "belongs_to",
            Some(1.0),
            Some("deterministic_text_ingestion"),
        );
        assert!(legacy_code_resolution(&belongs).is_none());

        let mut explicit = legacy_rel("calls", Some(0.9), Some("deterministic_text_ingestion"));
        explicit.code_resolution = Some(CodeEdgeResolution::new(
            ResolutionTier::Heuristic,
            producer::FRAMEWORK_PATTERN,
        ));
        let ev = legacy_code_resolution(&explicit).unwrap();
        assert_eq!(ev.resolved_by, producer::FRAMEWORK_PATTERN);
    }
}
