//! Tantivy-backed lexical retrieval index.
//!
//! This crate is the BM25 / full-text retrieval leg: it implements
//! [`engram_retrieval::RetrievalIndex`] over `KnowledgeChunk` text so
//! `RetrievalMode::Keyword` queries rank by term relevance instead of substring
//! containment. It is a focused adapter — Tantivy stays here, never in
//! `engram-domain` or `engram-retrieval` core — and it composes with the
//! existing vector and graph indexes through the shared fusion pipeline.
//!
//! Identifier-awareness lives in [`tokenizer::normalize_identifier_text`], a
//! pure pre-tokenization step, so Tantivy's default tokenizer handles the rest.

mod collector;
mod index;
mod retrieval;
mod tokenizer;

pub use collector::DistinctTargetCounters;
pub use index::{
    DOC_CONTRIBUTOR_KEY as LEXICAL_DOC_CONTRIBUTOR_KEY, LexicalIndex, code_contributor_key,
};
pub use retrieval::{LexicalResolvedTarget, LexicalRetrievalIndex, LexicalTargetResolver};
pub use tokenizer::normalize_identifier_text;
