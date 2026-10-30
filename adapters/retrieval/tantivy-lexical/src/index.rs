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
    query::{BooleanQuery, Occur, QueryParser, TermQuery},
    schema::{Field, STORED, STRING, Schema, TEXT, Value},
};

use crate::tokenizer::normalize_identifier_text;

/// On-disk format version of the lexical index (stamped as a `version` marker
/// file next to the Tantivy files).
///
/// Bump when the on-disk shape changes in a way an older/newer binary cannot
/// interpret consistently (schema field ids, normalization semantics that
/// change what a token means, storage layout). On mismatch [`LexicalIndex`]
/// discards the stale index and starts fresh (see [`open_with_compat`](
/// LexicalIndex::open_with_compat)) — silently serving a stale or
/// incompatible index after an upgrade is the failure this prevents.
///
/// v2 adds the `eligibility` contributor field (one document per
/// (target, contributor-key) pair) used for pre-limit eligibility collection
/// and targeted generation retirement.
pub const INDEX_FORMAT_VERSION: u32 = 2;

/// Contributor key stamped on ordinary (non-code) targets. Bound code targets
/// carry `code:<generation-id>` instead (see [`LexicalIndex::upsert_stamped`]).
pub const DOC_CONTRIBUTOR_KEY: &str = "doc";

/// Builds the bound-code contributor key for a generation id.
pub fn code_contributor_key(generation_id: &str) -> String {
    format!("code:{generation_id}")
}

/// Marker file name stamped into the index directory.
const VERSION_MARKER: &str = "version";
/// Tantivy's own metadata file — its presence means a real index exists.
const TANTIVY_META: &str = "meta.json";

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
    eligibility_field: Field,
}

impl LexicalIndex {
    /// Builds the fixed lexical schema and returns it with the typed field
    /// handles derived from it. Used by both [`new`] and [`open`] so the field
    /// ids stay identical across the in-RAM and file-backed constructors — the
    /// persisted-on-disk schema is always byte-identical to the in-RAM one.
    ///
    /// [`new`]: Self::new
    /// [`open`]: Self::open
    fn build_schema() -> (Schema, Field, Field, Field) {
        let mut builder = Schema::builder();
        // TEXT: tokenized + indexed (BM25 ranking). The identifier-aware split
        // is applied before indexing via `normalize_identifier_text`.
        let text_field = builder.add_text_field("text", TEXT);
        // STRING: indexed but untokenized, so a delete Term matches the whole id;
        // STORED so the id survives into ranked results for rehydration.
        let target_id_field = builder.add_text_field("target_id", STRING | STORED);
        // STRING contributor key ("doc" or "code:<generation-id>"): one
        // document per (target, contributor) pair. The collector constrains
        // eligibility BEFORE the TopDocs limit; retirement deletes by this
        // term (see `retire_contributor`).
        let eligibility_field = builder.add_text_field("eligibility", STRING);
        (
            builder.build(),
            text_field,
            target_id_field,
            eligibility_field,
        )
    }

    /// Wires a writer + reader (with `OnCommitWithDelay`) onto an opened index.
    /// Shared by both constructors so the reload behavior is identical.
    fn attach_io(
        index: Index,
        text_field: Field,
        target_id_field: Field,
        eligibility_field: Field,
    ) -> tantivy::Result<Self> {
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
            eligibility_field,
        })
    }

    /// Creates an in-RAM lexical index for tests and ephemeral use.
    pub fn new() -> tantivy::Result<Self> {
        let (schema, text_field, target_id_field, eligibility_field) = Self::build_schema();
        let index = Index::create_in_ram(schema);
        Self::attach_io(index, text_field, target_id_field, eligibility_field)
    }

    /// Opens (or creates) a **file-backed** lexical index rooted at `directory`,
    /// discarding + rebuilding when the persisted index was written by an
    /// older format version.
    ///
    /// Returns the index plus `rebuilt: bool` — true when an existing index
    /// was DISCARDED because its `version` marker was missing or older than
    /// [`INDEX_FORMAT_VERSION`]. Callers surface the upgrade guidance (the
    /// rebuilt index is empty until repositories are re-scanned).
    ///
    /// All other behavior matches [`open`](Self::open) (create-if-missing,
    /// load-if-compatible, in-RAM fallback on hard failure).
    pub fn open_with_compat(directory: &Path) -> tantivy::Result<(Self, bool)> {
        let marker = directory.join(VERSION_MARKER);
        let stamped = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        let has_index = directory.join(TANTIVY_META).exists();

        if has_index && stamped != Some(INDEX_FORMAT_VERSION) {
            // Index on disk predates this format version (or carries no
            // marker). Serving it risks misinterpreted segments and stale
            // tokens; discard and start fresh — the next scan re-feeds.
            eprintln!(
                "engram-lexical: index at {} was built by an older format (marker {:?}, current v{INDEX_FORMAT_VERSION}); rebuilding empty — re-scan repositories to restore code-symbol search",
                directory.display(),
                stamped
            );
            if std::fs::remove_dir_all(directory).is_ok() {
                std::fs::create_dir_all(directory)?;
            }
            let index = Self::open_after_compat(directory)?;
            return Ok((index, true));
        }

        let index = Self::open_after_compat(directory)?;
        Ok((index, false))
    }

    /// Shared tail of the open paths: stamp the current version marker (idempotent)
    /// and open-or-create the Tantivy index.
    fn open_after_compat(directory: &Path) -> tantivy::Result<Self> {
        std::fs::create_dir_all(directory)?;
        if let Err(e) = std::fs::write(
            directory.join(VERSION_MARKER),
            INDEX_FORMAT_VERSION.to_string(),
        ) {
            eprintln!("engram-lexical: write version marker failed ({e}); continuing");
        }
        Self::open(directory)
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
        let (schema, text_field, target_id_field, eligibility_field) = Self::build_schema();

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
            Ok(index) => Self::attach_io(index, text_field, target_id_field, eligibility_field),
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

    /// Inserts or replaces the ordinary (non-code) document for `target_id`
    /// with normalized `text`, keyed to the [`DOC_CONTRIBUTOR_KEY`]
    /// contributor. Delete-then-add gives upsert idempotency keyed on
    /// `(target_id, contributor)`.
    pub fn upsert(&self, target_id: &str, text: &str) -> tantivy::Result<()> {
        self.upsert_stamped(target_id, text, DOC_CONTRIBUTOR_KEY)
    }

    /// Inserts or replaces the document for `target_id` under an explicit
    /// contributor key (`"doc"` or `"code:<generation-id>"`). One document
    /// per (target, contributor) pair: a target with both an ordinary and a
    /// bound-code contribution keeps both documents, so retiring one
    /// contributor never removes the other's tokens.
    pub fn upsert_stamped(
        &self,
        target_id: &str,
        text: &str,
        contributor_key: &str,
    ) -> tantivy::Result<()> {
        if contributor_key.trim().is_empty() {
            return Err(tantivy::TantivyError::InvalidArgument(
                "lexical contributor key must not be empty".to_owned(),
            ));
        }
        let normalized = normalize_identifier_text(text);
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        self.delete_contributor_doc(&mut writer, target_id, contributor_key)?;
        writer.add_document(doc!(
            self.target_id_field => target_id,
            self.text_field => normalized.as_str(),
            self.eligibility_field => contributor_key,
        ))?;
        writer.commit()?;
        Ok(())
    }

    /// Deletes the `(target_id, contributor_key)` document (no-op when
    /// absent) without committing — callers batch this with their writes.
    fn delete_contributor_doc(
        &self,
        writer: &mut IndexWriter,
        target_id: &str,
        contributor_key: &str,
    ) -> tantivy::Result<()> {
        use tantivy::query::{BooleanQuery, TermQuery};
        let identity = BooleanQuery::new(vec![
            (
                tantivy::query::Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.target_id_field, target_id),
                    tantivy::schema::IndexRecordOption::Basic,
                )) as Box<dyn tantivy::query::Query>,
            ),
            (
                tantivy::query::Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.eligibility_field, contributor_key),
                    tantivy::schema::IndexRecordOption::Basic,
                )) as Box<dyn tantivy::query::Query>,
            ),
        ]);
        writer.delete_query(Box::new(identity))?;
        Ok(())
    }

    /// Inserts or replaces many `(target_id, text)` entries in a single
    /// transaction (one commit), all keyed to the ordinary
    /// [`DOC_CONTRIBUTOR_KEY`] contributor.
    ///
    /// Equivalent to calling [`upsert`](Self::upsert) per entry, but commits
    /// only once at the end. Bulk indexing MUST use this, not per-document
    /// `upsert`: each `commit()` finalizes a Tantivy segment and (under
    /// `OnCommitWithDelay`) reloads the reader, so per-document commits make a
    /// full-corpus build O(n²) — e.g. ~18k entities took >90s one-commit-per-doc
    /// and froze the host process. Use `upsert` only for incremental
    /// single-document updates.
    pub fn upsert_batch(&self, entries: &[(String, String)]) -> tantivy::Result<()> {
        self.upsert_batch_stamped(
            entries
                .iter()
                .map(|(id, text)| (id.clone(), text.clone(), DOC_CONTRIBUTOR_KEY.to_owned()))
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    /// [`upsert_batch`](Self::upsert_batch) with explicit per-entry
    /// contributor keys.
    pub fn upsert_batch_stamped(
        &self,
        entries: &[(String, String, String)],
    ) -> tantivy::Result<()> {
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        for (target_id, text, contributor_key) in entries {
            if contributor_key.trim().is_empty() {
                return Err(tantivy::TantivyError::InvalidArgument(
                    "lexical contributor key must not be empty".to_owned(),
                ));
            }
            let normalized = normalize_identifier_text(text);
            self.delete_contributor_doc(&mut writer, target_id, contributor_key)?;
            writer.add_document(doc!(
                self.target_id_field => target_id.as_str(),
                self.text_field => normalized.as_str(),
                self.eligibility_field => contributor_key.as_str(),
            ))?;
        }
        writer.commit()?;
        Ok(())
    }

    /// Removes every contributor document for `target_id`, if present.
    pub fn delete(&self, target_id: &str) -> tantivy::Result<()> {
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        writer.delete_term(Term::from_field_text(self.target_id_field, target_id));
        writer.commit()?;
        Ok(())
    }

    /// Targeted warm-index retirement (task_014): removes EVERY document
    /// contributed by `contributor_key` (e.g. `code:<generation-id>`) and
    /// returns how many were removed — the durable receipt a generation
    /// retirement records. Documents the same target holds under OTHER
    /// contributor keys (an ordinary `doc` contribution, another surviving
    /// generation) are untouched, so a mixed-lineage target never loses its
    /// surviving tokens. The deletion commits atomically: after it returns,
    /// a reopened warm index can no longer serve the retired tokens.
    pub fn retire_contributor(&self, contributor_key: &str) -> tantivy::Result<usize> {
        use tantivy::{collector::Count, query::TermQuery};
        if contributor_key.trim().is_empty() {
            return Err(tantivy::TantivyError::InvalidArgument(
                "lexical contributor key must not be empty".to_owned(),
            ));
        }
        self.reader.reload()?;
        let term = Term::from_field_text(self.eligibility_field, contributor_key);
        let retired = self
            .reader
            .searcher()
            .search(
                &TermQuery::new(term.clone(), tantivy::schema::IndexRecordOption::Basic),
                &Count,
            )
            .unwrap_or(0);
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        writer.delete_term(term);
        writer.commit()?;
        Ok(retired)
    }

    /// Removes the one `(target, contributor)` document without touching the
    /// target's ordinary or surviving-generation contributions.
    pub fn retire_target_contributor(
        &self,
        target_id: &str,
        contributor_key: &str,
    ) -> tantivy::Result<usize> {
        if target_id.trim().is_empty() || contributor_key.trim().is_empty() {
            return Err(tantivy::TantivyError::InvalidArgument(
                "lexical target and contributor keys must not be empty".to_owned(),
            ));
        }
        self.reader.reload()?;
        let target = Term::from_field_text(self.target_id_field, target_id);
        let contributor = Term::from_field_text(self.eligibility_field, contributor_key);
        let query = BooleanQuery::new(vec![
            (
                Occur::Must,
                Box::new(TermQuery::new(
                    target.clone(),
                    tantivy::schema::IndexRecordOption::Basic,
                )),
            ),
            (
                Occur::Must,
                Box::new(TermQuery::new(
                    contributor.clone(),
                    tantivy::schema::IndexRecordOption::Basic,
                )),
            ),
        ]);
        let retired = self
            .reader
            .searcher()
            .search(&query, &tantivy::collector::Count)
            .unwrap_or(0);
        let mut writer = self.writer.lock().expect("lexical writer lock poisoned");
        writer.delete_query(Box::new(query))?;
        writer.commit()?;
        Ok(retired)
    }

    /// Returns `(target_id, bm25_score)` pairs ranked best-first, one hit per
    /// target (a target with several contributor documents keeps its
    /// best-scoring one).
    ///
    /// Query and indexed text are normalized identically, so identifier-style
    /// queries (`parseError`) match indexed identifiers. The reader is reloaded
    /// before each search so prior commits are visible (deterministic reads).
    pub fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<(String, f32)>> {
        self.search_contributors(query, limit, None)
    }

    /// Eligibility-constrained search: only documents whose contributor key
    /// is in `allowed_keys` participate in ranking, and the `limit` applies
    /// to the constrained candidate set — the constraint is inside the
    /// collector, BEFORE the TopDocs limit, so an ineligible high-scoring
    /// document cannot consume a slot an eligible target needs (REQ-005,
    /// task_013 seam).
    ///
    /// `allowed_keys` MUST be non-empty; an empty eligibility set selects
    /// nothing and is rejected rather than silently widening to all documents.
    pub fn search_within(
        &self,
        query: &str,
        limit: usize,
        allowed_keys: &std::collections::BTreeSet<String>,
    ) -> tantivy::Result<Vec<(String, f32)>> {
        if allowed_keys.is_empty() {
            return Err(tantivy::TantivyError::InvalidArgument(
                "constrained lexical search requires a non-empty eligibility set".to_owned(),
            ));
        }
        self.search_contributors(query, limit, Some(allowed_keys))
    }

    /// Canonical target + contributor constraint for bound retrieval
    /// (corrective r4, canonical 5). Target IDs come from the authoritative
    /// store resolver, not from a Tantivy contributor stamp; the eligibility
    /// contributor keys are intersected TOO, so a shared allowed target
    /// cannot be matched through documents an EXCLUDED generation
    /// contributed. Collection is DISTINCT-TARGET: one slot per target (its
    /// best matching document's score), so a flood of duplicate contributor
    /// documents for one target cannot consume the bounded page and starve
    /// another allowed target. The target budget applies to distinct targets
    /// BEFORE any document-budget semantics — no fixed overfetch, no
    /// post-filter.
    pub fn search_targets_within(
        &self,
        query: &str,
        limit: usize,
        target_ids: &std::collections::BTreeSet<String>,
        allowed_contributors: &std::collections::BTreeSet<String>,
    ) -> tantivy::Result<Vec<(String, f32)>> {
        self.search_targets_within_counted(query, limit, target_ids, allowed_contributors, None)
    }

    /// [`Self::search_targets_within`] with retained-state instrumentation
    /// (A5): the counters record the per-segment retained-map maximum (the
    /// documented transient K+1 bound) and the total fruit-merge input volume
    /// (each segment contributes at most K entries, so merge storage is
    /// O(segments × K) — never a global unbounded map). There is no hard scan
    /// cap; a future cap must fail explicitly rather than return a partial
    /// page.
    pub fn search_targets_within_counted(
        &self,
        query: &str,
        limit: usize,
        target_ids: &std::collections::BTreeSet<String>,
        allowed_contributors: &std::collections::BTreeSet<String>,
        counters: Option<&std::sync::Arc<crate::collector::DistinctTargetCounters>>,
    ) -> tantivy::Result<Vec<(String, f32)>> {
        use std::collections::BTreeSet;
        use tantivy::query::{BooleanQuery, TermSetQuery};
        if target_ids.is_empty() || allowed_contributors.is_empty() {
            return Ok(Vec::new());
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let normalized = normalize_identifier_text(query);
        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let parsed = parser.parse_query(&normalized)?;
        let target_terms: BTreeSet<Term> = target_ids
            .iter()
            .map(|id| Term::from_field_text(self.target_id_field, id))
            .collect();
        let contributor_terms: BTreeSet<Term> = allowed_contributors
            .iter()
            .map(|key| Term::from_field_text(self.eligibility_field, key))
            .collect();
        let query = BooleanQuery::new(vec![
            (tantivy::query::Occur::Must, Box::new(parsed)),
            (
                tantivy::query::Occur::Must,
                Box::new(TermSetQuery::new(target_terms)),
            ),
            (
                tantivy::query::Occur::Must,
                Box::new(TermSetQuery::new(contributor_terms)),
            ),
        ]);
        let mut collector =
            crate::collector::DistinctTargetCollector::new(limit, self.target_id_field);
        if let Some(counters) = counters {
            collector = collector.with_counters(std::sync::Arc::clone(counters));
        }
        let best = searcher.search(&query, &collector)?;
        let mut ranked: Vec<(String, f32)> = best.into_iter().collect();
        // Deterministic order: score desc, then target id asc.
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        ranked.truncate(limit);
        Ok(ranked)
    }

    /// Shared search path. `allowed_keys` adds a `TermSetQuery` over the
    /// `eligibility` field ANDed with the parsed user query (`None` = the
    /// historical unconstrained page).
    fn search_contributors(
        &self,
        query: &str,
        limit: usize,
        allowed_keys: Option<&std::collections::BTreeSet<String>>,
    ) -> tantivy::Result<Vec<(String, f32)>> {
        use std::collections::BTreeSet;
        use tantivy::query::{BooleanQuery, TermSetQuery};
        if limit == 0 {
            return Ok(Vec::new());
        }
        let normalized = normalize_identifier_text(query);
        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let parsed = query_parser.parse_query(&normalized)?;
        let boxed: Box<dyn tantivy::query::Query> = match allowed_keys {
            Some(keys) => {
                let terms: BTreeSet<Term> = keys
                    .iter()
                    .map(|key| Term::from_field_text(self.eligibility_field, key))
                    .collect();
                Box::new(BooleanQuery::new(vec![
                    (tantivy::query::Occur::Must, Box::new(parsed)),
                    (
                        tantivy::query::Occur::Must,
                        Box::new(TermSetQuery::new(terms)),
                    ),
                ]))
            }
            None => Box::new(parsed),
        };
        // Over-fetch contributor documents, then dedupe by target: several
        // contributor docs may reference the same target and the caller's
        // `limit` is a target budget, not a document budget.
        let fetch = limit.saturating_mul(2);
        let top = searcher.search(&boxed, &TopDocs::with_limit(fetch))?;

        let mut seen = BTreeSet::new();
        let mut out = Vec::with_capacity(limit);
        for (score, addr) in top {
            let doc = searcher.doc::<TantivyDocument>(addr)?;
            let id = doc
                .get_first(self.target_id_field)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_default();
            if seen.insert(id.clone()) {
                out.push((id, score));
                if out.len() == limit {
                    break;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constrained_search_ranks_within_the_eligible_set_before_the_limit() {
        // task_013 seam: a stale-generation document that outscores every
        // eligible target must not consume the constrained result page.
        let index = LexicalIndex::new().unwrap();
        index
            .upsert_stamped("stale", "parse parse parse parse parse", "code:gen-stale")
            .unwrap();
        index
            .upsert_stamped("eligible", "parse parse", "code:gen-1")
            .unwrap();
        index.upsert("doc-note", "parse").unwrap();

        let mut allowed: std::collections::BTreeSet<String> = Default::default();
        allowed.insert("doc".to_owned());
        allowed.insert("code:gen-1".to_owned());
        let hits = index.search_within("parse", 2, &allowed).unwrap();
        let ids: Vec<&str> = hits.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids.len(),
            2,
            "the page must fill from the eligible set: {ids:?}"
        );
        assert!(
            !ids.contains(&"stale"),
            "stale generation must never rank: {ids:?}"
        );
        assert!(ids.contains(&"eligible") && ids.contains(&"doc-note"));

        // The unconstrained page still ranks the stale document first —
        // proving the constraint, not scoring, removed it.
        let unconstrained = index.search("parse", 3).unwrap();
        assert_eq!(unconstrained[0].0, "stale");
    }

    #[test]
    fn constrained_search_rejects_empty_eligibility_and_keeps_contributor_docs_independent() {
        let index = LexicalIndex::new().unwrap();
        index
            .upsert_stamped("shared", "alpha parse", "code:gen-1")
            .unwrap();
        index.upsert_stamped("shared", "alpha", "doc").unwrap();
        let empty: std::collections::BTreeSet<String> = Default::default();
        assert!(index.search_within("alpha", 5, &empty).is_err());
        // Mixed contributor target survives a code-only constraint via the
        // doc contributor and a doc-only constraint finds it once.
        let mut docs_only: std::collections::BTreeSet<String> = Default::default();
        docs_only.insert("doc".to_owned());
        let hits = index.search_within("alpha", 5, &docs_only).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "shared");
    }

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

    /// Fresh open stamps the current format version; a matching re-open loads
    /// the persisted index WITHOUT rebuilding.
    #[test]
    fn open_with_compat_stamps_and_reuses_current_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");

        let (index, rebuilt) = LexicalIndex::open_with_compat(&lexical_dir).expect("open");
        assert!(!rebuilt, "fresh create is not a rebuild");
        index.upsert("t", "parse error").unwrap();
        // Tantivy allows one IndexWriter per directory: drop the handle before
        // re-opening (the persistence test uses the same pattern).
        drop(index);

        let marker = std::fs::read_to_string(lexical_dir.join("version")).unwrap();
        assert_eq!(marker.trim(), INDEX_FORMAT_VERSION.to_string());

        // Re-open with the current marker: loads persisted writes, no rebuild.
        let (index, rebuilt) = LexicalIndex::open_with_compat(&lexical_dir).expect("reopen");
        assert!(!rebuilt, "current-version index must be reused");
        let hits = index.search("parse", 10).unwrap();
        assert!(hits.iter().any(|(id, _)| id == "t"));
    }

    /// An index stamped by an OLDER version is discarded and rebuilt empty —
    /// the upgrade guard. Without it, an upgraded binary would silently serve
    /// a stale/incompatible index.
    #[test]
    fn open_with_compat_rebuilds_stale_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lexical_dir = dir.path().join("lexical");

        // Build a real index (meta.json + writes), then stamp it OLD.
        let (index, _) = LexicalIndex::open_with_compat(&lexical_dir).expect("open");
        index.upsert("t", "parse error").unwrap();
        drop(index);
        std::fs::write(lexical_dir.join("version"), "0").unwrap();

        let (rebuilt_index, rebuilt) = LexicalIndex::open_with_compat(&lexical_dir).expect("open");
        assert!(rebuilt, "stale-version index must be rebuilt");
        // The rebuilt index is empty — old contents discarded, not served.
        let hits = rebuilt_index.search("parse", 10).unwrap();
        assert!(hits.is_empty(), "stale contents must not survive: {hits:?}");
        drop(rebuilt_index);
        // And the marker is current again (a third open is a plain reuse).
        let (_, rebuilt_again) = LexicalIndex::open_with_compat(&lexical_dir).expect("open");
        assert!(!rebuilt_again);
    }
}
