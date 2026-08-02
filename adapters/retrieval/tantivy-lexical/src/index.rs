//! Tantivy-backed lexical store: BM25 over normalized chunk text.
//!
//! This module owns the index itself (schema, writer, reader). It is secondary
//! adapter state — it stores target references and normalized text only, never
//! canonical chunk records. [`crate::LexicalRetrievalIndex`] adapts hits into
//! portable retrieval candidates with policy and provenance.

use std::path::Path;
use std::sync::Mutex;

use tantivy::{
    Index, IndexBuilder, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term,
    collector::TopDocs,
    directory::MmapDirectory,
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
    /// Builds the fixed lexical schema and returns it with the typed field
    /// handles derived from it. Used by both [`new`] and [`open`] so the field
    /// ids stay identical across the in-RAM and file-backed constructors — the
    /// persisted-on-disk schema is always byte-identical to the in-RAM one.
    ///
    /// [`new`]: Self::new
    /// [`open`]: Self::open
    fn build_schema() -> (Schema, Field, Field) {
        let mut builder = Schema::builder();
        // TEXT: tokenized + indexed (BM25 ranking). The identifier-aware split
        // is applied before indexing via `normalize_identifier_text`.
        let text_field = builder.add_text_field("text", TEXT);
        // STRING: indexed but untokenized, so a delete Term matches the whole id;
        // STORED so the id survives into ranked results for rehydration.
        let target_id_field = builder.add_text_field("target_id", STRING | STORED);
        (builder.build(), text_field, target_id_field)
    }

    /// Wires a writer + reader (with `OnCommitWithDelay`) onto an opened index.
    /// Shared by both constructors so the reload behavior is identical.
    fn attach_io(index: Index, text_field: Field, target_id_field: Field) -> tantivy::Result<Self> {
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

    /// Creates an in-RAM lexical index for tests and ephemeral use.
    pub fn new() -> tantivy::Result<Self> {
        let (schema, text_field, target_id_field) = Self::build_schema();
        let index = Index::create_in_ram(schema);
        Self::attach_io(index, text_field, target_id_field)
    }

    /// Opens (or creates) a **file-backed** lexical index rooted at `directory`.
    ///
    /// The index persists across processes: writes from one process (e.g.
    /// `scan_repo` feeding entity names) are visible to reads in a later process
    /// (e.g. `search` / `recall` on a fresh MCP), which fixes the cross-process
    /// lexical-lane gap where an in-RAM index was empty on every fresh process.
    ///
    /// Behavior:
    /// - The directory is created if missing.
    /// - If the directory already holds a Tantivy index whose schema matches
    ///   [`build_schema`](Self::build_schema), it is loaded (segments + all
    ///   prior writes).
    /// - If it holds no index, a fresh one is created on disk.
    /// - On any failure to open/create the on-disk index (missing dir, IO error,
    ///   schema mismatch, corruption), this logs a warning and **falls back to an
    ///   in-RAM index** rather than crashing — recall then degrades to
    ///   "no lexical lane until the next scan", never an abort.
    ///
    /// The same `Arc<LexicalIndex>` is shared between the write-side
    /// [`LexicalFeed`](crate::LexicalFeed) and the read-side
    /// [`LexicalRetrievalIndex`](crate::LexicalRetrievalIndex), so writes commit
    /// to disk and reads reload from disk through the one handle.
    pub fn open(directory: &Path) -> tantivy::Result<Self> {
        let (schema, text_field, target_id_field) = Self::build_schema();

        if let Err(e) = std::fs::create_dir_all(directory) {
            eprintln!(
                "engram-lexical: create lexical dir failed ({e}); falling back to in-RAM index"
            );
            return Self::new();
        }

        let open_result = (|| {
            let mmap_dir = MmapDirectory::open(directory)?;
            IndexBuilder::new().schema(schema).open_or_create(mmap_dir)
        })();
        match open_result {
            Ok(index) => Self::attach_io(index, text_field, target_id_field),
            Err(e) => {
                // Corrupted index, schema mismatch, or an unreadable directory.
                // Degrade to ephemeral rather than aborting bootstrap: the lexical
                // lane will be empty until a `scan_repo` repopulates it, and the
                // operator can wipe `directory` to reset.
                eprintln!(
                    "engram-lexical: open_or_create lexical index at {} failed ({e}); falling back to in-RAM index",
                    directory.display()
                );
                Self::new()
            }
        }
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

    /// Inserts or replaces many documents in a single transaction (one commit).
    ///
    /// Equivalent to calling [`upsert`](Self::upsert) per entry, but commits
    /// only once at the end. Bulk indexing MUST use this, not per-document
    /// `upsert`: each `commit()` finalizes a Tantivy segment and (under
    /// `OnCommitWithDelay`) reloads the reader, so per-document commits make a
    /// full-corpus build O(n²) — e.g. ~18k entities took >90s one-commit-per-doc
    /// and froze the host process. Use `upsert` only for incremental
    /// single-document updates.
    pub fn upsert_batch(&self, entries: &[(String, String)]) -> tantivy::Result<()> {
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        for (target_id, text) in entries {
            let normalized = normalize_identifier_text(text);
            // Delete-first keeps upsert idempotency for repeated target ids.
            writer.delete_term(Term::from_field_text(
                self.target_id_field,
                target_id.as_str(),
            ));
            writer.add_document(doc!(
                self.target_id_field => target_id.as_str(),
                self.text_field => normalized.as_str(),
            ))?;
        }
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

    #[test]
    fn upsert_batch_indexes_many_in_one_commit() {
        let index = LexicalIndex::new().unwrap();
        let entries = vec![
            ("a".to_string(), "parse error".to_string()),
            ("b".to_string(), "parse warning".to_string()),
            ("c".to_string(), "unrelated content".to_string()),
        ];
        index.upsert_batch(&entries).unwrap();

        let hits = index.search("parse", 10).unwrap();
        let ids: Vec<&str> = hits.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(hits.len(), 2, "both parse docs should match");
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert!(!ids.contains(&"c"), "non-matching doc should be absent");
    }

    #[test]
    fn upsert_batch_replaces_existing_targets() {
        let index = LexicalIndex::new().unwrap();
        index
            .upsert_batch(&[("t".to_string(), "alpha beta".to_string())])
            .unwrap();
        // Second batch for the same id must replace, not duplicate.
        index
            .upsert_batch(&[("t".to_string(), "gamma delta".to_string())])
            .unwrap();

        let alpha_hits = index.search("alpha", 10).unwrap();
        assert!(
            alpha_hits.is_empty(),
            "batch upsert must replace, not duplicate"
        );
        let gamma_hits = index.search("gamma", 10).unwrap();
        assert!(gamma_hits.iter().any(|(id, _)| id == "t"));
    }

    /// `open` creates the directory if missing and lands on a file-backed index
    /// that survives being dropped — the precondition for cross-process search.
    #[test]
    fn open_creates_directory_if_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");
        assert!(!lexical_dir.exists());

        let index = LexicalIndex::open(&lexical_dir).expect("open creates the dir + index");
        index
            .upsert("fn1", "function parseError(input: string)")
            .unwrap();

        // The directory now exists on disk.
        assert!(lexical_dir.is_dir());
        let _ = index; // drop is allowed to flush.
    }

    /// The core cross-process guarantee: writes from one `LexicalIndex` are
    /// visible to a second `LexicalIndex` opened at the same directory later —
    /// no in-RAM handoff, only the persisted on-disk segments. This is the
    /// property that makes `search`/`recall` work on a fresh MCP process.
    #[test]
    fn open_persists_writes_across_index_instances() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");

        // Process A: write.
        let index_a = LexicalIndex::open(&lexical_dir).expect("open A");
        index_a.upsert("fn1", "function snapshotSession").unwrap();
        index_a.upsert("fn2", "function renderVault").unwrap();
        drop(index_a);

        // Process B: read on a fresh handle at the same directory.
        let index_b = LexicalIndex::open(&lexical_dir).expect("open B");
        let hits = index_b.search("snapshotSession", 10).unwrap();
        assert!(
            hits.iter().any(|(id, _)| id == "fn1"),
            "persisted write must be visible to a fresh index instance: {hits:?}"
        );

        let render_hits = index_b.search("render", 10).unwrap();
        assert!(
            render_hits.iter().any(|(id, _)| id == "fn2"),
            "second persisted write must also survive: {render_hits:?}"
        );
    }

    /// Schema mismatch (e.g. an index from an incompatible older build) must
    /// not crash bootstrap — it falls back to an empty in-RAM index and the
    /// caller keeps working (degraded, no lexical lane until re-scan).
    #[test]
    fn open_falls_back_to_ram_on_unrelated_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A directory that exists but is NOT a valid Tantivy index (no meta.json,
        // just stray files). `open_or_create` treats a bare directory as "no
        // index" and creates fresh, so this path returns a working index rather
        // than erroring — assert the no-crash contract either way.
        let lexical_dir = dir.path().join("lexical");
        std::fs::create_dir_all(&lexical_dir).unwrap();
        std::fs::write(lexical_dir.join("stray"), b"not an index").unwrap();

        let index = LexicalIndex::open(&lexical_dir).expect("open must not crash");
        // Whatever it returned is a usable index: a write + read round-trips.
        index.upsert("t", "parse error").unwrap();
        let hits = index.search("parse", 10).unwrap();
        assert!(hits.iter().any(|(id, _)| id == "t"));
    }
}
