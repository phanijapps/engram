//! Engine-neutral pagination primitives.
//!
//! `Cursor` is an opaque, serde-transparent token that an adapter interprets (the
//! SQLite adapter encodes a `rowid`; a future pgvector adapter its own position). The
//! type itself carries no SQL or engine knowledge — it stays out of `engram-domain`'s
//! neutrality contract (ADR-0022). `Page<T>` is the paged-result envelope every paged
//! read port returns, so keyset pagination lives behind one canonical, engine-neutral
//! shape instead of leaking into callers (it replaces the TS `node:sqlite` keyset that
//! lived in the viz BFF).

use serde::{Deserialize, Serialize};

/// Opaque pagination cursor. Adapters encode their own seek position; callers treat it
/// as an opaque string and hand it back unchanged to fetch the next page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(String);

impl Cursor {
    /// Wrap an adapter-specific seek token (e.g. base64url of a SQLite `rowid`).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// The opaque token string (for adapter decode / JSON transport).
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Consume into the underlying string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for Cursor {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// A page of results: the items + the cursor to fetch the next page (`None` at the end).
///
/// Serialized as `{ "items": [...], "nextCursor": "<opaque>" | null }` (camelCase) so the
/// wire shape matches existing TS consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<Cursor>) -> Self {
        Self { items, next_cursor }
    }

    /// A terminal page (no further results).
    pub fn last(items: Vec<T>) -> Self {
        Self { items, next_cursor: None }
    }

    /// Map the items, preserving the cursor (e.g. record → view projection).
    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_opaque_string_on_the_wire() {
        let c = Cursor::new("abc123");
        let json = serde_json::to_string(&c).expect("serialize");
        assert_eq!(json, "\"abc123\""); // transparent → plain string, not an object
        let back: Cursor = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, c);
    }

    #[test]
    fn page_serializes_camel_case() {
        let page: Page<i32> = Page::new(vec![1, 2, 3], Some(Cursor::new("next")));
        let json = serde_json::to_string(&page).expect("serialize");
        assert!(json.contains("\"items\""));
        assert!(json.contains("\"nextCursor\":\"next\""));
    }

    #[test]
    fn terminal_page_has_null_cursor() {
        let page: Page<i32> = Page::last(vec![1]);
        let json = serde_json::to_string(&page).expect("serialize");
        assert!(json.contains("\"nextCursor\":null"));
    }

    #[test]
    fn page_map_preserves_cursor() {
        let page: Page<i32> = Page::new(vec![1, 2], Some(Cursor::new("c")));
        let mapped = page.map(|x| x * 10);
        assert_eq!(mapped.items, vec![10, 20]);
        assert_eq!(mapped.next_cursor, Some(Cursor::new("c")));
    }
}
