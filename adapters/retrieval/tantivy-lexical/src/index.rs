//! Tantivy-backed lexical store: BM25 over normalized chunk text.
//!
//! This module owns the index itself (schema, writer, reader). It is secondary
//! adapter state — it stores target references and normalized text only, never
//! canonical chunk records. [`crate::LexicalRetrievalIndex`] adapts hits into
//! portable retrieval candidates with policy and provenance.

use std::sync::Mutex;

use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT, Value},
};

use crate::tokenizer::normalize_identifier_text;

/// Lexical index backed by Tantivy.
///
/// Indexes normalized text and returns BM25-ranked target references. The
/// Tantivy handle, schema, and tokenizer live here; ingest feeding lands in T4b
/// and retrieval-candidate shaping in [`crate::retrieval`].
pub struct LexicalIndex {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    text_field: Field,
    target_id_field: Field,
}

impl LexicalIndex {
    /// Creates an in-RAM lexical index for tests and ephemeral use.
    pub fn new() -> tantivy::Result<Self> {
        let mut builder = Schema::builder();
        // TEXT: tokenized + indexed (BM25 ranking). The identifier-aware split
        // is applied before indexing via `normalize_identifier_text`.
        let text_field = builder.add_text_field("text", TEXT);
        // STRING: indexed but untokenized, so a delete Term matches the whole id;
        // STORED so the id survives into ranked results for rehydration.
        let target_id_field = builder.add_text_field("target_id", STRING | STORED);
        let schema = builder.build();

        let index = Index::create_in_ram(schema);
        let writer = index.writer(50_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            text_field,
            target_id_field,
        })
    }

    /// Inserts or replaces the document for `target_id` with normalized `text`.
    ///
    /// Delete-then-add gives upsert idempotency keyed on `target_id`.
    pub fn upsert(&self, target_id: &str, text: &str) -> tantivy::Result<()> {
        let normalized = normalize_identifier_text(text);
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        writer.delete_term(Term::from_field_text(self.target_id_field, target_id));
        writer.add_document(doc!(
            self.target_id_field => target_id,
            self.text_field => normalized.as_str(),
        ))?;
        writer.commit()?;
        Ok(())
    }

    /// Removes the document for `target_id`, if present.
    pub fn delete(&self, target_id: &str) -> tantivy::Result<()> {
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        writer.delete_term(Term::from_field_text(self.target_id_field, target_id));
        writer.commit()?;
        Ok(())
    }

    /// Returns `(target_id, bm25_score)` pairs ranked best-first.
    ///
    /// Query and indexed text are normalized identically, so identifier-style
    /// queries (`parseError`) match indexed identifiers. The reader is reloaded
    /// before each search so prior commits are visible (deterministic reads).
    pub fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<(String, f32)>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let normalized = normalize_identifier_text(query);
        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let parsed = query_parser.parse_query(&normalized)?;
        let top = searcher.search(&parsed, &TopDocs::with_limit(limit))?;

        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc = searcher.doc::<TantivyDocument>(addr)?;
            let id = doc
                .get_first(self.target_id_field)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_default();
            out.push((id, score));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_ranks_by_term_frequency_and_length() {
        let index = LexicalIndex::new().unwrap();
        index.upsert("alpha", "parse parse parse").unwrap();
        index.upsert("beta", "parse").unwrap();
        index
            .upsert("gamma", "parse other other other other other")
            .unwrap();

        let hits = index.search("parse", 3).unwrap();
        let ids: Vec<&str> = hits.iter().map(|(id, _)| id.as_str()).collect();
        // alpha: tf=3, short field -> highest. beta: tf=1, short -> above gamma.
        // gamma: tf=1, long field -> lowest.
        assert_eq!(ids, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn identifier_queries_match_indexed_identifiers() {
        let index = LexicalIndex::new().unwrap();
        index
            .upsert("fn1", "function parseError(input: string)")
            .unwrap();

        let parse_hits = index.search("parse", 10).unwrap();
        assert!(parse_hits.iter().any(|(id, _)| id == "fn1"));

        let snake_hits = index.search("parse_error", 10).unwrap();
        assert!(snake_hits.iter().any(|(id, _)| id == "fn1"));
    }

    #[test]
    fn delete_removes_a_chunk() {
        let index = LexicalIndex::new().unwrap();
        index.upsert("keep", "alpha bravo charlie").unwrap();
        index.upsert("drop", "alpha bravo delta").unwrap();

        let before = index.search("alpha", 10).unwrap();
        assert_eq!(before.len(), 2);

        index.delete("drop").unwrap();
        let after = index.search("alpha", 10).unwrap();
        let ids: Vec<&str> = after.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["keep"]);
    }

    #[test]
    fn upsert_replaces_existing_target() {
        let index = LexicalIndex::new().unwrap();
        index.upsert("t1", "parse error").unwrap();
        index.upsert("t1", "completely different content").unwrap();
        let hits = index.search("parse", 10).unwrap();
        assert!(hits.is_empty(), "upsert must replace, not duplicate");
    }
}
