//! Rule-based entity anchor extraction for memory write enrichment.
//!
//! Extracts capitalised-token-sequence anchors from memory text and populates
//! `MemoryContent.entities` so the cue retrieval path has a backing store.
//! See ADR-0015 for why anchors live on `MemoryContent.entities` rather than a
//! separate table.

use engram_domain::EntityRef;

const PUNCT: &[char] = &[
    '.', ',', ';', ':', '!', '?', '\'', '"', '(', ')', '[', ']', '{', '}',
];
const MAX_EXTRACTED: usize = 20;

/// Extracts entity anchors from `text` using a capitalised-token-sequence
/// heuristic and merges them with any caller-supplied entities.
///
/// Extraction rules (see ADR-0015 §Behavior):
/// - Tokenise by whitespace; trim `PUNCT` from each token boundary.
/// - Tokens that trim to empty break the run.
/// - Runs of ≥ 2 consecutive non-empty trimmed tokens each starting with a
///   Unicode uppercase letter become one `EntityRef { kind: "unknown", … }`.
/// - Deduplicate within extracted results by `name.to_lowercase()` (first
///   occurrence wins); cap to the 20 longest; ties broken by document order.
/// - Merge with `caller` entities: caller wins on `name.to_lowercase()` match;
///   entities with `name: None` bypass name-dedup and are kept as-is.
pub fn extract(text: &str) -> Vec<EntityRef> {
    trim_extracted(raw_runs(text))
}

/// Merges `extracted` anchors with caller-supplied `caller` entities.
///
/// Caller entries win on `name.to_lowercase()` conflict. Entities with
/// `name: None` bypass name-dedup and are always kept.
pub fn merge_entities(extracted: Vec<EntityRef>, caller: Vec<EntityRef>) -> Vec<EntityRef> {
    let caller_names: std::collections::HashSet<String> = caller
        .iter()
        .filter_map(|e| e.name.as_deref())
        .map(|n| n.to_lowercase())
        .collect();

    let mut merged: Vec<EntityRef> = caller;
    for entity in extracted {
        match &entity.name {
            Some(name) if !caller_names.contains(&name.to_lowercase()) => merged.push(entity),
            None => merged.push(entity),
            _ => {} // caller entry wins
        }
    }
    merged
}

// ---------- internals --------------------------------------------------------

/// Finds all capitalised-token-sequence runs in `text` and returns them as
/// raw `EntityRef` values (before dedup/cap).
fn raw_runs(text: &str) -> Vec<EntityRef> {
    let mut results = Vec::new();
    let mut current_run: Vec<&str> = Vec::new();

    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(PUNCT);
        if trimmed.is_empty() {
            // Empty after trim — breaks the run.
            flush_run(&mut current_run, &mut results);
        } else if trimmed.chars().next().map_or(false, |c| c.is_uppercase()) {
            current_run.push(trimmed);
        } else {
            flush_run(&mut current_run, &mut results);
        }
    }
    flush_run(&mut current_run, &mut results);
    results
}

fn flush_run(run: &mut Vec<&str>, out: &mut Vec<EntityRef>) {
    if run.len() >= 2 {
        let name = run.join(" ");
        out.push(EntityRef {
            id: None,
            kind: Some("unknown".to_owned()),
            name: Some(name),
            aliases: Vec::new(),
        });
    }
    run.clear();
}

/// Deduplicates `entities` within themselves by `name.to_lowercase()` (first
/// occurrence wins), then caps to `MAX_EXTRACTED` keeping the longest names
/// (ties broken by document order, i.e. stable sort position).
fn trim_extracted(mut entities: Vec<EntityRef>) -> Vec<EntityRef> {
    // Dedup within extracted (first occurrence wins).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    entities.retain(|e| {
        if let Some(name) = &e.name {
            seen.insert(name.to_lowercase())
        } else {
            true
        }
    });

    // Cap to MAX_EXTRACTED by longest name, ties broken by document order.
    if entities.len() > MAX_EXTRACTED {
        // Stable sort by descending char count preserves original order on ties.
        entities.sort_by(|a, b| {
            let la = a.name.as_deref().map_or(0, |n| n.chars().count());
            let lb = b.name.as_deref().map_or(0, |n| n.chars().count());
            lb.cmp(&la)
        });
        entities.truncate(MAX_EXTRACTED);
    }

    entities
}

// ---------- tests ------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(name: &str) -> EntityRef {
        EntityRef {
            id: None,
            kind: Some("unknown".to_owned()),
            name: Some(name.to_owned()),
            aliases: Vec::new(),
        }
    }

    fn caller_entity(name: &str, kind: &str) -> EntityRef {
        EntityRef {
            id: None,
            kind: Some(kind.to_owned()),
            name: Some(name.to_owned()),
            aliases: Vec::new(),
        }
    }

    // --- raw extraction ---

    #[test]
    fn two_cap_tokens_extracted() {
        let result = extract("Project Orion is on track");
        assert_eq!(result, vec![entity("Project Orion")]);
    }

    #[test]
    fn two_cap_tokens_kind_is_unknown() {
        let result = extract("Dave Smith approved the PR");
        assert_eq!(result, vec![entity("Dave Smith")]);
    }

    #[test]
    fn unicode_uppercase_extracted() {
        let result = extract("Élise Martin joined");
        assert_eq!(result, vec![entity("Élise Martin")]);
    }

    #[test]
    fn no_proper_nouns_empty() {
        assert!(extract("no proper nouns here").is_empty());
    }

    #[test]
    fn single_cap_token_not_extracted() {
        // "Single" is one capitalised token; "capitalised" starts lowercase.
        assert!(extract("Single capitalised word").is_empty());
    }

    #[test]
    fn punctuation_trimmed_at_boundary() {
        // Comma on "Smith," is trimmed; run forms.
        let result = extract("Dave Smith, the lead");
        assert_eq!(result, vec![entity("Dave Smith")]);
    }

    #[test]
    fn non_uppercase_token_breaks_run() {
        // "&" is not uppercase so it falls through the else branch and breaks the run.
        assert!(extract("Alpha & Beta joined").is_empty());
    }

    #[test]
    fn empty_after_trim_breaks_run() {
        // "..." trims to empty (all chars in PUNCT), breaking the run.
        assert!(extract("Alpha ... Beta joined").is_empty());
    }

    #[test]
    fn empty_text_empty_result() {
        assert!(extract("").is_empty());
    }

    #[test]
    fn multiple_runs_extracted() {
        let result = extract("Alice Chen met Bob Ross at the Project Atlas kickoff");
        let names: Vec<_> = result.iter().filter_map(|e| e.name.as_deref()).collect();
        assert!(
            names.contains(&"Alice Chen"),
            "expected Alice Chen in {names:?}"
        );
        assert!(
            names.contains(&"Bob Ross"),
            "expected Bob Ross in {names:?}"
        );
        assert!(
            names.contains(&"Project Atlas"),
            "expected Project Atlas in {names:?}"
        );
    }

    #[test]
    fn all_kinds_are_unknown() {
        let result = extract("Dave Smith joined Project Orion");
        for e in &result {
            assert_eq!(e.kind.as_deref(), Some("unknown"));
        }
    }

    // --- cap ---

    #[test]
    fn cap_at_twenty_longest() {
        // 5 long runs (15+ chars each) + 20 short runs (6 chars each) = 25 total.
        // Cap should keep the 5 long + 15 short (longest 20 survive).
        let long_runs: String = (0u32..5)
            .map(|i| format!("Longrun{i:02} Verylongname{i:02} and "))
            .collect();
        let short_runs: String = (0u32..20)
            .map(|i| format!("Ab{i:02} Cd{i:02} and "))
            .collect();
        let text = long_runs + &short_runs;
        let result = extract(&text);
        assert_eq!(result.len(), MAX_EXTRACTED, "exactly 20 returned");
        // All 5 long runs must survive (they are longest by char count).
        for i in 0u32..5 {
            let expected = format!("Longrun{i:02} Verylongname{i:02}");
            assert!(
                result
                    .iter()
                    .any(|e| e.name.as_deref() == Some(expected.as_str())),
                "long run {expected} should survive cap"
            );
        }
    }

    // --- within-extracted dedup ---

    #[test]
    fn within_extraction_dedup_case_insensitive() {
        // "Project Orion" appears twice; only first survives.
        let result = extract("Project Orion and Project Orion again");
        let count = result
            .iter()
            .filter(|e| e.name.as_deref() == Some("Project Orion"))
            .count();
        assert_eq!(count, 1);
    }

    // --- merge ---

    #[test]
    fn caller_entity_preserved_no_duplicate() {
        let caller = vec![caller_entity("Orion", "project")];
        // extract yields "Project Orion"; merge should keep caller "Orion" and add "Project Orion".
        let extracted = vec![entity("Project Orion"), entity("Orion")];
        let merged = merge_entities(extracted, caller);
        // "orion" matches caller → extracted "Orion" dropped; "Project Orion" added.
        let names: Vec<_> = merged.iter().filter_map(|e| e.name.as_deref()).collect();
        assert!(names.contains(&"Orion"), "caller Orion should survive");
        assert!(
            names.contains(&"Project Orion"),
            "Project Orion should be added"
        );
        let orion_count = names
            .iter()
            .filter(|&&n| n.to_lowercase() == "orion")
            .count();
        assert_eq!(orion_count, 1, "no duplicate Orion");
    }

    #[test]
    fn none_named_entity_always_kept() {
        let caller = vec![EntityRef {
            id: None,
            kind: Some("custom".to_owned()),
            name: None,
            aliases: Vec::new(),
        }];
        let extracted = vec![entity("Project Orion")];
        let merged = merge_entities(extracted, caller);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|e| e.name.is_none()));
    }

    #[test]
    fn caller_wins_on_conflict() {
        let caller = vec![caller_entity("Project Orion", "project")];
        let extracted = vec![entity("Project Orion")]; // extracted same name
        let merged = merge_entities(extracted, caller);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].kind.as_deref(), Some("project")); // caller kind survives
    }
}
