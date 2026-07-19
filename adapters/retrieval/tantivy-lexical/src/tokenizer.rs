//! Identifier-aware text normalization for lexical indexing.
//!
//! Tantivy's default tokenizer splits on non-alphanumeric boundaries, which
//! leaves `snake_case` intact and never splits `camelCase` / `PascalCase`.
//! Source identifiers need both. Rather than implement a custom Tantivy
//! `Tokenizer` trait, this module normalizes text *before* it reaches the index:
//! splitting camelCase / PascalCase, underscores, and other non-alphanumeric
//! boundaries into spaces and lowercasing. Tantivy's tokenizer then tokenizes
//! the normalized space-delimited text. The function is pure and unit-tested.

/// Splits identifier-style text into lowercase, space-delimited tokens.
///
/// `parseError`, `parse_error`, and `parse-error` all normalize to
/// `parse error`, so a `parse` query matches each.
pub fn normalize_identifier_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut prev_lower_or_digit = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            // camelCase / PascalCase boundary: an uppercase following a lowercase
            // letter or digit starts a new token.
            if ch.is_ascii_uppercase() && prev_lower_or_digit {
                out.push(' ');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            // Any non-alphanumeric (underscore, hyphen, punctuation, whitespace)
            // is a token separator.
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            prev_lower_or_digit = false;
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_camel_case() {
        assert_eq!(normalize_identifier_text("parseError"), "parse error");
    }

    #[test]
    fn splits_snake_and_kebab() {
        assert_eq!(normalize_identifier_text("parse_error"), "parse error");
        assert_eq!(normalize_identifier_text("parse-error"), "parse error");
    }

    #[test]
    fn all_three_share_parse_token() {
        // The spec AC: parseError / parse_error / a `parse` query all normalize
        // to contain the `parse` token, so they match.
        for id in ["parseError", "parse_error", "parse"] {
            assert!(
                normalize_identifier_text(id)
                    .split_whitespace()
                    .any(|t| t == "parse"),
                "{id} should normalize to contain the `parse` token"
            );
        }
    }

    #[test]
    fn collapses_whitespace_and_lowercases() {
        assert_eq!(
            normalize_identifier_text("  Hello   WORLD  "),
            "hello world"
        );
    }
}
