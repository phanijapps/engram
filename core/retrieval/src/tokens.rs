//! Query-token policy for the heuristic retrieval lanes.
//!
//! The associative (PPR seeding) and community-summary (token match gate)
//! lanes tokenize the raw query with simple splitting. That made three
//! classes of token drive matching when they never should (reproduced
//! 2026-10-20, search-quality audit):
//!
//! 1. **English function words** — "to" is a substring of `related_to` (the
//!    near-universal predicate), so nearly every natural-language query
//!    ("how to X") passed the community match gate and resurrected the
//!    deterministic lowest-label noise; "how to" seeded `Tokenizer` via
//!    containment in the associative lane.
//! 2. **1-char tokens** — "c" (from "c++") is a substring of "community".
//! 3. **Template words** — the community summary template itself contributed
//!    "Community:"/"edges:" to the matchable haystack (fixed at the call
//!    site by matching names+predicates only, not here).
//!
//! One policy, one place: [`query_tokens`] is the tokenizer the heuristic
//! lanes use. The lexical (BM25) and vector lanes are untouched — Tantivy
//! and the embedder apply their own (statistical) weighting.

/// English function words dropped from heuristic-lane query tokens. Common
/// in natural-language queries, meaningless for identifier matching.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "nor", "so", "yet", "for", "if", "then", "than",
    "because", "while", "when", "where", "who", "whom", "whose", "why", "how", "what", "which",
    "that", "this", "these", "those", "is", "are", "was", "were", "be", "been", "being", "am",
    "do", "does", "did", "doing", "have", "has", "had", "will", "would", "shall", "should", "can",
    "could", "may", "might", "must", "not", "no", "yes", "of", "in", "on", "at", "by", "to",
    "into", "onto", "from", "with", "without", "within", "about", "over", "under", "above",
    "below", "between", "through", "during", "before", "after", "until", "against", "up", "down",
    "out", "off", "again", "further", "once", "here", "there", "all", "any", "both", "each", "few",
    "more", "most", "other", "some", "such", "only", "own", "same", "too", "very", "just", "also",
    "it", "its", "as", "related",
];

/// Minimum token length for heuristic matching — 1-char tokens match far
/// too widely by substring ("c" ⊂ "community").
const MIN_TOKEN_LEN: usize = 2;

/// Heuristic-lane query tokens: lowercased, alphanumeric-trimmed, at least
/// [`MIN_TOKEN_LEN`] chars, with English function words dropped. Returns an
/// empty vec when nothing meaningful remains — callers treat that as "no
/// heuristic match" (empty seeds / closed match gate), never as
/// match-everything.
pub fn query_tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|t| t.len() >= MIN_TOKEN_LEN && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_function_words_and_short_tokens() {
        assert_eq!(query_tokens("how to run"), vec!["run"]);
        assert_eq!(query_tokens("a b c"), Vec::<String>::new());
        assert_eq!(
            query_tokens("what does the tokenizer do"),
            vec!["tokenizer"]
        );
        // "to" inside related_to / "related" cannot drive matching.
        assert_eq!(query_tokens("related to"), Vec::<String>::new());
    }

    #[test]
    fn keeps_identifier_tokens_verbatim_lowercased() {
        assert_eq!(query_tokens("parse_error"), vec!["parse_error"]);
        assert_eq!(query_tokens("ParseError"), vec!["parseerror"]);
        assert_eq!(query_tokens("_tokens"), vec!["tokens"]);
        assert_eq!(query_tokens("Tokenizer"), vec!["tokenizer"]);
    }

    #[test]
    fn punctuation_is_trimmed_not_split() {
        // "node.js" stays one token (inner dot preserved by trim_matches).
        assert_eq!(query_tokens("node.js"), vec!["node.js"]);
        assert_eq!(query_tokens("'quoted'"), vec!["quoted"]);
    }
}

/// Deterministic context-size estimator for `ContextBudget.max_tokens`
/// enforcement (MCCL-audit F3): one token ≈ 4 characters, rounded up.
///
/// This is a documented APPROXIMATION shared by every transport — not a model
/// tokenizer. It exists so budget semantics are engine- and model-neutral and
/// reproducible in fixtures; hosts needing exact model accounting inject their
/// own estimator at the presentation layer and lower the budget accordingly.
pub fn estimate_context_tokens(text: &str) -> u32 {
    text.chars().count().div_ceil(4) as u32
}

#[cfg(test)]
mod estimate_tests {
    use super::estimate_context_tokens;

    #[test]
    fn rounds_up_and_counts_chars_not_bytes() {
        assert_eq!(estimate_context_tokens(""), 0);
        assert_eq!(estimate_context_tokens("abcd"), 1);
        assert_eq!(estimate_context_tokens("abcde"), 2);
        // Multi-byte chars count as one char each (é = 2 bytes, 1 char).
        assert_eq!(estimate_context_tokens("éééé"), 1);
    }
}
