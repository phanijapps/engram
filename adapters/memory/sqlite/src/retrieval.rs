//! SQL-backed keyword and cue retrieval baseline.
//!
//! Dispatches on `request.modes` to run keyword scoring, cue matching, or both.
//! All paths apply the same scope, policy, lifecycle, and budget guards before
//! producing results.

use std::collections::BTreeSet;

use engram_domain::*;
use engram_memory::{CoreError, CoreResult};

use crate::{
    engine::SqlMemoryService, scope::scope_allows, validation::validate_retrieval_request,
};

// ---------- public entry point -----------------------------------------------

pub(crate) async fn retrieve(
    service: &SqlMemoryService,
    request: RetrievalRequest,
) -> CoreResult<ContextPayload> {
    validate_retrieval_request(&request)?;
    let now = service.clock.now();
    let include_explanations = request.include_explanations.unwrap_or(false);
    let max_items = effective_max_items(&request);
    let records = service.store.list_memories()?;

    // `modes` is the sole dispatch authority.
    let keyword_active =
        request.modes.is_empty() || request.modes.contains(&RetrievalMode::Keyword);
    let cue_active = request.modes.contains(&RetrievalMode::Cue);

    let terms = if keyword_active {
        query_terms(&request.query)
    } else {
        Vec::new()
    };

    struct Candidate {
        total: f32,
        kw_score: Option<f32>,
        kw_terms: Vec<String>,
        cue: Option<CueMatch>,
        record: MemoryRecord,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut omitted = Vec::new();

    for record in records {
        // --- policy gauntlet (unchanged) ---
        if !scope_allows(&record.scope, &request.scope) {
            continue;
        }
        if !memory_filter_allows(&record, request.filters.as_ref()) {
            continue;
        }
        if let Some(expires_at) = record.policy.expires_at
            && expires_at <= now
        {
            omitted.push(omitted_result(&record, OmittedReason::Expired));
            continue;
        }
        if matches!(
            record.status,
            MemoryStatus::Redacted | MemoryStatus::Forgotten
        ) {
            omitted.push(omitted_result(&record, OmittedReason::Redacted));
            continue;
        }
        if matches!(record.status, MemoryStatus::Archived)
            && !request
                .filters
                .as_ref()
                .and_then(|f| f.include_archived)
                .unwrap_or(false)
        {
            continue;
        }
        if !record.policy.allowed_uses.is_empty()
            && !record.policy.allowed_uses.contains(&AllowedUse::Retrieval)
        {
            omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
            continue;
        }
        if let Err(error) =
            service
                .authorizer
                .can_retrieve(&request.requester, &record.scope, &record.policy)
        {
            if matches!(error, CoreError::PolicyDenied { .. }) {
                omitted.push(omitted_result(&record, OmittedReason::PolicyDenied));
                continue;
            }
            return Err(error);
        }
        // --- end gauntlet ---

        let kw = keyword_active
            .then(|| keyword_score(&record, &request.query, &terms))
            .flatten();

        let cm = if cue_active {
            let m = cue_score(&record, &request.cues);
            (m.score > 0.0).then_some(m)
        } else {
            None
        };

        if kw.is_none() && cm.is_none() {
            continue;
        }

        let kw_score = kw.as_ref().map(|(s, _)| *s);
        let cm_score = cm.as_ref().map(|m| m.score);
        let total = kw_score.unwrap_or(0.0).max(cm_score.unwrap_or(0.0));
        let kw_terms = kw.map(|(_, t)| t).unwrap_or_default();

        candidates.push(Candidate {
            total,
            kw_score,
            kw_terms,
            cue: cm,
            record,
        });
    }

    candidates.sort_by(|a, b| {
        b.total
            .total_cmp(&a.total)
            .then_with(|| b.record.created_at.cmp(&a.record.created_at))
            .then_with(|| a.record.id.cmp(&b.record.id))
    });

    let mut items = Vec::new();
    for (index, c) in candidates.into_iter().enumerate() {
        if index >= max_items {
            omitted.push(omitted_result(&c.record, OmittedReason::BudgetExceeded));
            continue;
        }
        items.push(build_result(
            index,
            c.total,
            c.kw_score,
            c.kw_terms,
            c.cue,
            c.record,
            include_explanations,
        ));
    }

    Ok(ContextPayload {
        items,
        budget: request.budget,
        omitted,
        source_failures: Vec::new(),
        created_at: now,
    })
}

// ---------- cue matching -----------------------------------------------------

struct CueMatch {
    score: f32,
    matched: Vec<Cue>,
}

fn cue_score(record: &MemoryRecord, cues: &[Cue]) -> CueMatch {
    let recognized: Vec<&Cue> = cues
        .iter()
        .filter(|c| c.slot == "entity" || c.slot == "kind")
        .collect();

    if recognized.is_empty() {
        return CueMatch {
            score: 0.0,
            matched: Vec::new(),
        };
    }

    let mut matched = Vec::new();
    for &cue in &recognized {
        let hit = record
            .content
            .entities
            .iter()
            .any(|entity| match cue.slot.as_str() {
                "entity" => entity
                    .name
                    .as_deref()
                    .map(|n| string_op_matches(n, &cue.value, &cue.operator))
                    .unwrap_or(false),
                "kind" => entity
                    .kind
                    .as_deref()
                    .map(|k| {
                        // kind:None entities are skipped for kind-slot cues (handled by unwrap_or)
                        string_op_matches(k, &cue.value, &cue.operator)
                    })
                    .unwrap_or(false),
                _ => false,
            });
        if hit {
            matched.push(cue.clone());
        }
    }

    // denominator is recognized.len(); non-string values and unsupported
    // operators count as unmatched and stay in the denominator.
    let score = matched.len() as f32 / recognized.len() as f32;
    CueMatch { score, matched }
}

/// Applies a string-valued cue operator. Returns false for non-string `value`
/// (stays in the denominator, counts as unmatched). Unsupported operators
/// (In, Range, Exists) also return false.
fn string_op_matches(field: &str, value: &Scalar, operator: &Option<CueOperator>) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    let target = raw.trim();
    // Empty or whitespace-only target would over-match Contains/StartsWith/EndsWith.
    if target.is_empty() {
        return false;
    }
    let f = field.to_lowercase();
    let t = target.to_lowercase();
    match operator {
        None | Some(CueOperator::Equals) => f == t,
        Some(CueOperator::Contains) => f.contains(t.as_str()),
        Some(CueOperator::StartsWith) => f.starts_with(t.as_str()),
        Some(CueOperator::EndsWith) => f.ends_with(t.as_str()),
        // In, Range, Exists — unimplemented; count as unmatched in denominator.
        _ => false,
    }
}

// ---------- result building --------------------------------------------------

enum MatchMode {
    KeywordOnly,
    CueOnly,
    Both,
}

impl MatchMode {
    fn source(&self) -> &'static str {
        match self {
            Self::KeywordOnly => "sql.memory.keyword",
            Self::CueOnly => "sql.memory.cue",
            Self::Both => "sql.memory.keyword+cue",
        }
    }
    fn fusion_strategy(&self) -> FusionStrategy {
        match self {
            Self::Both => FusionStrategy::MaxScore,
            _ => FusionStrategy::None,
        }
    }
    fn source_score(&self, total: f32, kw: f32, cue: f32) -> f32 {
        match self {
            Self::KeywordOnly => kw,
            Self::CueOnly => cue,
            Self::Both => total,
        }
    }
    fn rerank_score(&self, total: f32) -> Option<f32> {
        match self {
            Self::KeywordOnly => Some(total),
            _ => None,
        }
    }
    fn rerank_strategy(&self) -> Option<RerankStrategy> {
        match self {
            Self::KeywordOnly => Some(RerankStrategy::None),
            _ => None,
        }
    }
    fn reason(&self) -> &'static str {
        match self {
            Self::KeywordOnly => "Matched memory content with SQL keyword retrieval.",
            Self::CueOnly => "Matched memory entity anchors with SQL cue retrieval.",
            Self::Both => "Matched memory content and entity anchors.",
        }
    }
}

fn build_result(
    index: usize,
    total: f32,
    kw_score: Option<f32>,
    kw_terms: Vec<String>,
    cm: Option<CueMatch>,
    record: MemoryRecord,
    include_explanation: bool,
) -> RetrievalResult {
    let mode = match (&kw_score, &cm) {
        (Some(_), Some(_)) => MatchMode::Both,
        (None, Some(_)) => MatchMode::CueOnly,
        _ => MatchMode::KeywordOnly,
    };

    let kw = kw_score.unwrap_or(0.0);
    let cue = cm.as_ref().map(|m| m.score).unwrap_or(0.0);
    let matched_cues = cm.as_ref().map(|m| m.matched.clone()).unwrap_or_default();
    let cm_score = cm.map(|m| m.score);

    let explanation = include_explanation.then(|| RetrievalExplanation {
        reason: mode.reason().to_owned(),
        matched_cues,
        matched_terms: kw_terms,
        path: Vec::new(),
        source_summary: record.content.summary.clone(),
    });

    RetrievalResult {
        id: format!("result-{}", record.id),
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        content: record.content.text,
        score: RetrievalScore {
            total,
            relevance: kw_score,
            recency: None,
            confidence: record.provenance.confidence,
            cue_match: cm_score,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: record.provenance,
        policy: record.policy,
        explanation,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: mode.source().to_owned(),
            source_rank: Some((index + 1) as u32),
            source_score: Some(mode.source_score(total, kw, cue)),
            score: None,
            rank: None,
            fusion_strategy: Some(mode.fusion_strategy()),
            fusion_score: Some(total),
            rerank_strategy: mode.rerank_strategy(),
            rerank_score: mode.rerank_score(total),
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

// ---------- helpers ----------------------------------------------------------

fn effective_max_items(request: &RetrievalRequest) -> usize {
    let limit = request.limit.unwrap_or(u32::MAX);
    let budget_limit = request
        .budget
        .as_ref()
        .and_then(|budget| budget.max_items)
        .unwrap_or(u32::MAX);
    limit.min(budget_limit) as usize
}

fn memory_filter_allows(record: &MemoryRecord, filters: Option<&QueryFilter>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    if !filters.memory_kinds.is_empty() && !filters.memory_kinds.contains(&record.kind) {
        return false;
    }
    if let Some(since) = filters.since
        && record.created_at < since
    {
        return false;
    }
    if let Some(until) = filters.until
        && record.created_at > until
    {
        return false;
    }
    if let Some(min_confidence) = filters.min_confidence
        && record.provenance.confidence.unwrap_or(0.0) < min_confidence
    {
        return false;
    }
    true
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let term = term.trim().to_lowercase();
            (!term.is_empty()).then_some(term)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn keyword_score(
    record: &MemoryRecord,
    query: &str,
    terms: &[String],
) -> Option<(f32, Vec<String>)> {
    let content = searchable_content(record);
    let normalized_query = query.trim().to_lowercase();
    let exact_match = content.contains(&normalized_query);
    let matched_terms = terms
        .iter()
        .filter(|term| content.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !exact_match && matched_terms.is_empty() {
        return None;
    }

    let term_score = if terms.is_empty() {
        0.0
    } else {
        matched_terms.len() as f32 / terms.len() as f32
    };
    let relevance = if exact_match {
        1.0_f32.max(term_score)
    } else {
        term_score
    };
    let confidence = record.provenance.confidence.unwrap_or(1.0);
    Some((
        ((relevance * 0.85) + (confidence * 0.15)).min(1.0),
        matched_terms,
    ))
}

fn searchable_content(record: &MemoryRecord) -> String {
    let mut content = record.content.text.to_lowercase();
    if let Some(summary) = &record.content.summary {
        content.push(' ');
        content.push_str(&summary.to_lowercase());
    }
    content
}

fn omitted_result(record: &MemoryRecord, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: RetrievalTargetType::Memory,
        target_id: record.id.to_string(),
        reason,
    }
}

// ---------- tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_entity(name: &str) -> EntityRef {
        EntityRef {
            id: None,
            kind: Some("unknown".to_owned()),
            name: Some(name.to_owned()),
            aliases: Vec::new(),
        }
    }

    fn make_entity_with_kind(name: &str, kind: &str) -> EntityRef {
        EntityRef {
            id: None,
            kind: Some(kind.to_owned()),
            name: Some(name.to_owned()),
            aliases: Vec::new(),
        }
    }

    fn stub_record(entities: Vec<EntityRef>) -> MemoryRecord {
        use chrono::Utc;
        let now = Utc::now();
        let scope = Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        };
        let actor = Actor {
            id: Id::from("a"),
            kind: ActorKind::System,
            display_name: None,
            metadata: None,
        };
        let provenance = Provenance {
            source: "test".to_owned(),
            actor: actor.clone(),
            observed_at: now,
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: None,
        };
        let policy = Policy {
            visibility: Visibility::Private,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: Vec::new(),
            expires_at: None,
            delete_mode: None,
        };
        MemoryRecord {
            id: Id::from("mem-1"),
            kind: MemoryKind::Observation,
            content: MemoryContent {
                text: "test content".to_owned(),
                summary: None,
                entities,
                language: None,
                format: None,
                structured: None,
                hash: None,
            },
            scope,
            provenance,
            policy,
            status: MemoryStatus::Active,
            links: Vec::new(),
            assertions: Vec::new(),
            created_at: now,
            updated_at: None,
            metadata: None,
        }
    }

    fn make_cue(slot: &str, value: serde_json::Value, op: Option<CueOperator>) -> Cue {
        Cue {
            slot: slot.to_owned(),
            value,
            operator: op,
            weight: None,
        }
    }

    // --- cue_score unit tests ---

    #[test]
    fn cue_score_entity_contains() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![make_cue(
            "entity",
            json!("Orion"),
            Some(CueOperator::Contains),
        )];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
        assert_eq!(m.matched.len(), 1);
    }

    #[test]
    fn cue_score_partial_match() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![
            make_cue("entity", json!("Orion"), Some(CueOperator::Contains)),
            make_cue("entity", json!("Nobody"), Some(CueOperator::Equals)),
        ];
        let m = cue_score(&record, &cues);
        assert!((m.score - 0.5).abs() < f32::EPSILON);
        assert_eq!(m.matched.len(), 1);
    }

    #[test]
    fn cue_score_kind_equals() {
        let record = stub_record(vec![make_entity_with_kind("Alice", "person")]);
        let cues = vec![make_cue("kind", json!("person"), Some(CueOperator::Equals))];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_empty_entities() {
        let record = stub_record(vec![]);
        let cues = vec![make_cue(
            "entity",
            json!("Orion"),
            Some(CueOperator::Equals),
        )];
        let m = cue_score(&record, &cues);
        assert_eq!(m.score, 0.0);
    }

    #[test]
    fn cue_score_starts_with() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![make_cue(
            "entity",
            json!("Pro"),
            Some(CueOperator::StartsWith),
        )];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_ends_with() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![make_cue(
            "entity",
            json!("orion"),
            Some(CueOperator::EndsWith),
        )];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_none_operator_defaults_to_equals() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![make_cue("entity", json!("Project Orion"), None)];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_non_string_value_stays_in_denominator() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        // One matching string cue + one non-string value → 0.5
        let cues = vec![
            make_cue("entity", json!("Orion"), Some(CueOperator::Contains)),
            make_cue("entity", json!(42), Some(CueOperator::Equals)),
        ];
        let m = cue_score(&record, &cues);
        assert!((m.score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_unsupported_operator_stays_in_denominator() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        // One matching + one In operator → 0.5
        let cues = vec![
            make_cue("entity", json!("Orion"), Some(CueOperator::Contains)),
            make_cue("entity", json!("Orion"), Some(CueOperator::In)),
        ];
        let m = cue_score(&record, &cues);
        assert!((m.score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_unknown_slot_excluded_from_denominator() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        // One recognized matching + one unknown slot → 1.0 (unknown excluded)
        let cues = vec![
            make_cue("entity", json!("Orion"), Some(CueOperator::Contains)),
            make_cue("tag", json!("anything"), Some(CueOperator::Equals)),
        ];
        let m = cue_score(&record, &cues);
        assert!((m.score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_all_unknown_slots_short_circuits() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cues = vec![make_cue(
            "tag",
            json!("anything"),
            Some(CueOperator::Equals),
        )];
        let m = cue_score(&record, &cues);
        assert_eq!(m.score, 0.0);
    }

    #[test]
    fn cue_score_empty_cues_short_circuits() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let m = cue_score(&record, &[]);
        assert_eq!(m.score, 0.0);
    }

    #[test]
    fn cue_score_weight_ignored() {
        let record = stub_record(vec![make_entity("Project Orion")]);
        let cue_no_weight = make_cue("entity", json!("Orion"), Some(CueOperator::Contains));
        let mut cue_with_weight = cue_no_weight.clone();
        cue_with_weight.weight = Some(99.0);
        let m1 = cue_score(&record, &[cue_no_weight]);
        let m2 = cue_score(&record, &[cue_with_weight]);
        assert!((m1.score - m2.score).abs() < f32::EPSILON);
    }

    #[test]
    fn cue_score_kind_none_entity_skipped_for_kind_slot() {
        // Entity has kind:None — should not match a kind-slot cue.
        let entity = EntityRef {
            id: None,
            kind: None,
            name: Some("Alice".to_owned()),
            aliases: Vec::new(),
        };
        let record = stub_record(vec![entity]);
        let cues = vec![make_cue("kind", json!("person"), Some(CueOperator::Equals))];
        let m = cue_score(&record, &cues);
        assert_eq!(m.score, 0.0);
    }

    #[test]
    fn cue_score_empty_string_value_does_not_match() {
        // Empty and whitespace-only targets must not over-match.
        let record = stub_record(vec![make_entity("Project Orion")]);
        for bad_value in [json!(""), json!(" "), json!("\t")] {
            for op in [
                CueOperator::Contains,
                CueOperator::StartsWith,
                CueOperator::EndsWith,
                CueOperator::Equals,
            ] {
                let cues = vec![make_cue("entity", bad_value.clone(), Some(op.clone()))];
                let m = cue_score(&record, &cues);
                assert_eq!(
                    m.score, 0.0,
                    "value {bad_value:?} with {op:?} should not match"
                );
            }
        }
    }
}
