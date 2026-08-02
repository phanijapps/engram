//! Code-intelligence tools (RFC-0015 Phase 2 + 3): `scan_repo` + the composites
//! + `search` + `get_context`.
//!
//! `scan_repo` uses a fan-in adapter so treesitter ingestion routes through
//! the provider's handles (no engine-store bypass).

use engram_domain::{
    KnowledgeEntity, KnowledgeRelationship, RetrievalRequest, RetrievalResult,
    RetrievalSourceFailure, RetrievalTargetType,
};
use engram_ingest::{
    KnowledgeRepoGraph, ScanFilter, ScanFilterConfig, ScanOptions, scan_repository,
};
use futures::executor::block_on;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::app::App;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::{internal, policy, req_str, requester, system_actor};

/// Default per-direction visited cap for bounded code-graph neighborhoods. The
/// depth defaults (`symbol_context`=1, `change_impact`=2) are the primary flood
/// bound; this cap is the safety net for raised-depth or super-hub queries.
const DEFAULT_NEIGHBORHOOD_CAP: usize = 64;

/// Per-item excerpt cap for `get_context` recall text. A single class-level
/// chunk can be thousands of lines; bounding item COUNT (the `limit` arg, up to
/// 50) does not bound SIZE. Each item is excerpted to this many chars.
const CONTEXT_ITEM_EXCERPT_CHARS: usize = 2000;

/// Total joined-recall cap for `get_context`. Once the assembled recall text
/// reaches this size, remaining items are dropped and a budget-reached note is
/// appended. Prevents 58k-token context bloat.
const CONTEXT_TOTAL_CHAR_BUDGET: usize = 50_000;

/// Default minimum fused score for `search` results (Fix 5). Items below this
/// are barely-relevant noise that bloats context. Overridable via the
/// `min_score` arg.
const DEFAULT_MIN_SCORE: f32 = 0.01;

/// Maximum results kept per source file in `search` (Fix 3: diversity). Caps
/// the "5 methods from the same file" case; over-cap items are dropped with a
/// per-file note.
const MAX_PER_FILE: usize = 2;

/// Excerpt `content` to at most `max_chars` Unicode scalar values, appending a
/// `[truncated]` marker when cut. Slices on a char boundary so multi-byte UTF-8
/// never panics. Returns the content unchanged when it already fits.
fn excerpt(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_owned();
    }
    let end = content
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(content.len());
    format!("{}\n... [truncated]", &content[..end])
}

/// Reduce a caller-supplied repository spelling to the `org/name` core that
/// reliably appears inside a git-remote provenance source. Strips an optional
/// leading host (`github.com/`, `github.com:`, `git@github.com/`,
/// `git@github.com:`) so a fully-qualified name matches the SSH-remote form
/// embedded by `scan_repo` (`engram-mcp-scan [git@github.com:org/name.git@...]`).
/// Lowercased and trimmed. Returns the empty string when the input has no
/// `org/name` core (e.g. a bare host like `"github.com"`), so a degenerate
/// filter never matches every GitHub-sourced row.
fn normalize_repository_key(repository: &str) -> String {
    let lower = repository.trim().to_ascii_lowercase();
    let prefixes = [
        "github.com/",
        "github.com:",
        "git@github.com/",
        "git@github.com:",
    ];
    let core = prefixes
        .iter()
        .find_map(|p| lower.strip_prefix(p))
        .unwrap_or(&lower);
    if core.contains('/') {
        core.to_owned()
    } else {
        String::new()
    }
}

/// True when `provenance_source` originates from `repository`. Matching is
/// intentionally flexible: a caller may pass `"phanijapps/engram"`,
/// `"github.com/phanijapps/engram"`, or `"git@github.com:phanijapps/engram"`,
/// and any of them must match a source like
/// `engram-mcp-scan [git@github.com:phanijapps/engram.git@feat/x:sha]`. Both
/// sides are lowercased; the repository is reduced to its `org/name` core (see
/// [`normalize_repository_key`]) before a substring test against the source. An
/// empty key (bare host / empty input) matches nothing.
fn source_matches_repository(source: &str, repository: &str) -> bool {
    let needle = normalize_repository_key(repository);
    if needle.is_empty() {
        return false;
    }
    source.to_ascii_lowercase().contains(&needle)
}

/// Diagnostic appended when a `repository` filter removes every result, so a
/// cross-repo filter never silently returns an empty packet.
fn repository_empty_note(repo: &str) -> String {
    format!(
        "(filtered to repository '{repo}'; 0 matches. Try without repository filter or a different repository.)"
    )
}

/// Extracts a concise `org/repo` label from a provenance `source` string (the
/// field `scan_repo` populates). Git-backed sources embed the remote inside
/// brackets, e.g. `"engram-mcp-scan [git@github.com:org/repo.git@branch:sha]"`;
/// this pulls out `"org/repo"`. Falls back to the bare source name (before the
/// `[`) when no recognizable git remote is embedded, so a non-git source still
/// renders something meaningful. The result is lowercased (it derives from the
/// lowercased remote) — acceptable for a display label.
fn provenance_repo_label(source: &str) -> String {
    let inner = source
        .split_once('[')
        .map(|(_, rest)| rest.trim_end_matches(']'))
        .unwrap_or(source);
    let lower = inner.to_ascii_lowercase();
    // Strip a known host prefix and take up to ".git@" (the remote/sha boundary).
    for sep in ["github.com/", "github.com:"] {
        if let Some(rest) = lower.split_once(sep).map(|(_, r)| r) {
            let repo = rest
                .split_once(".git@")
                .or_else(|| rest.split_once(' '))
                .map(|(r, _)| r)
                .unwrap_or(rest);
            if repo.contains('/') {
                return repo.to_owned();
            }
        }
    }
    // Fallback: the source name before the bracket.
    source
        .split_once('[')
        .map(|(name, _)| name.trim())
        .unwrap_or(source.trim())
        .to_owned()
}

/// Renders a provenance bracket for one recall item: `[retriever, score=X.XX]`.
/// Used as a per-item suffix in `search` and `get_context` output so a caller
/// can see WHERE each result came from without leaving the result line.
fn recall_provenance_suffix(item: &RetrievalResult) -> String {
    let retriever = item
        .fusion_trace
        .as_ref()
        .map(|t| t.source.as_str())
        .unwrap_or("?");
    format!("[{retriever}, score={:.2}]", item.score.total)
}

/// Builds the concise diagnostics appended after "No results." when recall
/// returned zero usable Entity hits. Surfaces (1) how many raw items recall
/// produced before the Entity filter, (2) how many Entity hits survived (after
/// dedup), (3) how many lanes contributed, and (4) any lane that errored. This
/// turns a silent empty packet into an actionable signal.
fn format_no_results_diag(
    total_recall: usize,
    entity_hits: usize,
    lanes: usize,
    failures: &[RetrievalSourceFailure],
) -> String {
    let mut parts = vec![
        format!("recall returned {total_recall} items"),
        format!("{entity_hits} entity hits after filter+dedup"),
        format!("{lanes} lanes contributed"),
    ];
    if !failures.is_empty() {
        let errored: Vec<String> = failures
            .iter()
            .map(|f| format!("{} ({})", f.source, f.reason))
            .collect();
        parts.push(format!("errored: {}", errored.join(", ")));
    }
    parts.join("; ")
}

/// `scan_repo`: treesitter-index a code repository into the project workspace,
/// routed through the provider via the fan-in adapter. Feeds code-symbol names
/// to the lexical lane so `search`/`recall` find them.
pub fn scan_repo(app: &App, args: &Value) -> Result<Value, ToolError> {
    let path = req_str(args, "path")?;
    let knowledge = app.provider.require_knowledge().map_err(internal)?.clone();
    let graph = app.provider.require_graph().map_err(internal)?.clone();
    let repo = KnowledgeRepoGraph::new(knowledge, graph);

    let (scan_filter, filter_note) = resolve_scan_filter(path, args);
    let opts = ScanOptions {
        scope: app.scope.clone(),
        policy: policy(),
        actor: system_actor(),
        source_name: "engram-mcp-scan".to_owned(),
        max_bytes: 0,
        manifest: HashMap::new(),
        scan_filter,
    };
    let (summary, _manifest) =
        scan_repository(std::path::Path::new(path), &opts, &repo, |_| ()).map_err(internal)?;

    // The scan just wrote entities + relationships for this scope. Invalidate
    // the graph snapshot cache for this scope so the next `search`/`recall`
    // reloads fresh data instead of serving a stale pre-scan snapshot. Without
    // this, a re-scan would leave recall answering from the old graph.
    if let Some(cache) = app.provider.graph_cache() {
        block_on(cache.invalidate(&app.scope));
    }

    // Feed code-symbol names to the lexical lane so keyword search finds them.
    if let (Ok(query), Ok(feed)) = (
        app.provider.require_knowledge_query(),
        app.provider.require_lexical_feed(),
    ) {
        let entries: Vec<(String, String)> = block_on(query.list_entities(&app.scope))
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.id.to_string(), format!("{} {:?}", e.name, e.kind)))
            .collect();
        if !entries.is_empty() {
            if let Err(e) = block_on(feed.upsert_batch(&entries)) {
                // Non-fatal: search may return no code symbols, but the scan itself succeeded.
                eprintln!("engram-mcp: lexical feed warning: {e}");
            }
        }
    }

    // Embed chunks into the vector index (when fastembed is wired).
    //
    // Incremental + batched (indexing-embed-performance): only chunks whose id
    // is NOT already in the vector index are embedded, and the embedding work
    // runs in batches through `embed_batch` (one FastEmbed model call per batch
    // instead of one per chunk). Re-scanning an unchanged repo embeds ~0 chunks;
    // adding a repo to a populated DB embeds only the new repo's chunks.
    let mut embedded = 0usize;
    if let (Ok(query), Some(embedder), Ok(vector_index)) = (
        app.provider.require_knowledge_query(),
        app.provider.embedding_provider(),
        app.provider.require_vectors(),
    ) {
        let chunks = block_on(query.list_chunks(&app.scope)).unwrap_or_default();
        let space = embedder.embedding_space();

        // Skip chunks that already have a vector — the incremental win. A
        // failed listing degrades to "embed everything" (current behavior).
        let have: std::collections::HashSet<engram_domain::Id> =
            block_on(vector_index.embedded_ids()).unwrap_or_default();
        let pending: Vec<&engram_domain::KnowledgeChunk> = chunks
            .iter()
            .filter(|c| !c.text.is_empty() && !have.contains(&c.id))
            .collect();

        const EMBED_BATCH_SIZE: usize = 64;
        for batch in pending.chunks(EMBED_BATCH_SIZE) {
            let texts: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();
            // Skip empty-text chunks defensively (the filter above already
            // dropped them) and warn per-chunk on insert errors, as before.
            match embedder.embed_batch(&texts) {
                Ok(vectors) => {
                    for (chunk, vector) in batch.iter().zip(vectors.into_iter()) {
                        if vector.is_empty() {
                            // Mirrors the skip-empty-text behavior for slots
                            // the batch path left empty.
                            continue;
                        }
                        if let Err(e) = block_on(vector_index.insert(&chunk.id, &space, vector)) {
                            eprintln!("engram-mcp: embed warning for {}: {e}", chunk.id);
                        } else {
                            embedded += 1;
                        }
                    }
                }
                Err(e) => {
                    // Whole batch failed — do not abort the scan; warn and move
                    // on so a single bad batch never blocks indexing.
                    for chunk in batch {
                        eprintln!("engram-mcp: embed error for {}: {e}", chunk.id);
                    }
                }
            }
        }
    }

    Ok(protocol::text_content(format!(
        "{summary:?}\n{filter_note}\nembedded {embedded} chunks"
    )))
}

/// Load + merge a scan filter from a JSON config file.
fn try_load_scan_filter(path: &Path) -> Result<ScanFilter, String> {
    let json =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg =
        ScanFilterConfig::from_json(&json).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(ScanFilter::merge(&cfg))
}

/// Resolve the scan filter via the discovery ladder:
///   1. an explicit `scan_config` path arg, else
///   2. `<repo>/.engram/scan.json` if it exists, else
///   3. the builtin defaults.
///
/// Returns the filter + a short note for the result text. Never errors — a
/// missing/malformed config soft-fails to the builtin so a bad config file
/// never aborts a scan.
fn resolve_scan_filter(repo_root: &str, args: &Value) -> (ScanFilter, String) {
    // (1) explicit arg wins.
    if let Some(explicit) = args.get("scan_config").and_then(|v| v.as_str()) {
        return match try_load_scan_filter(Path::new(explicit)) {
            Ok(filter) => (filter, format!("scan_config applied: {explicit}")),
            Err(e) => (
                ScanFilter::default(),
                format!("scan_config ignored ({e}); builtin filter"),
            ),
        };
    }
    // (2) repo-local <root>/.engram/scan.json. Read directly (no `exists()`
    // probe) so a transient removal between probe and read can't produce a
    // misleading "ignored" note; NotFound simply means no config.
    let discovered = Path::new(repo_root).join(".engram").join("scan.json");
    match std::fs::read_to_string(&discovered) {
        Ok(json) => match ScanFilterConfig::from_json(&json) {
            Ok(cfg) => {
                return (
                    ScanFilter::merge(&cfg),
                    ".engram/scan.json applied".to_owned(),
                );
            }
            Err(e) => {
                return (
                    ScanFilter::default(),
                    format!(".engram/scan.json ignored (parse {e}); builtin filter"),
                );
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* no config; fall through */ }
        Err(e) => {
            return (
                ScanFilter::default(),
                format!(".engram/scan.json ignored (read {e}); builtin filter"),
            );
        }
    }
    // (3) builtin.
    (
        ScanFilter::default(),
        "no scan config; builtin filter".to_owned(),
    )
}

// --- composites (T2–T8) ------------------------------------------------------

/// Fetch all relationships in the project scope. Errors (rather than returning
/// an empty vec) when the knowledge-query capability is unwired or the store
/// fails — so graph tools fail loudly instead of masquerading as "no relations".
pub(crate) fn fetch_rels(app: &App) -> Result<Vec<KnowledgeRelationship>, ToolError> {
    let query = app
        .provider
        .require_knowledge_query()
        .map_err(|_| internal("knowledge query capability not configured"))?;
    block_on(query.list_relationships(&app.scope))
        .map_err(|e| internal(format!("knowledge query failed: {e}")))
}

/// In-scope relationships, preferring the shared graph snapshot cache (the same
/// materialized edge set the recall graph lanes — associative-PPR and
/// community-summary — traverse) and falling back to a direct store read when
/// the cache is cold. Using the snapshot keeps the structural
/// `[Code]`/`[Graph]` sections of `get_context` consistent with the edge set
/// recall just served.
fn graph_relationships(app: &App) -> Result<Vec<KnowledgeRelationship>, ToolError> {
    if let Some(cache) = app.provider.graph_cache()
        && let Some(snap) = block_on(cache.get(&app.scope))
    {
        return Ok(snap.relationships.clone());
    }
    fetch_rels(app)
}

/// [`graph_relationships`] narrowed to a single repository when `repository` is
/// `Some`, else the full in-scope set. Filtering the edge set BEFORE running
/// `symbol_context` / link extraction is the high-leverage graph pre-filter:
/// non-target-repo symbols no longer compete in the neighborhood walk. Each
/// relationship carries its own `provenance.source`, so a cross-repo edge (e.g.
/// an engram→zbot `describes` link) is excluded by the filter.
fn graph_relationships_filtered(
    app: &App,
    repository: Option<&str>,
) -> Result<Vec<KnowledgeRelationship>, ToolError> {
    let rels = graph_relationships(app)?;
    match repository {
        Some(repo) if !normalize_repository_key(repo).is_empty() => Ok(rels
            .into_iter()
            .filter(|r| source_matches_repository(&r.provenance.source, repo))
            .collect()),
        _ => Ok(rels),
    }
}

// --- accurate + compact retrieval helpers (Fixes 1–5) -----------------------

/// Resolve the source file path for an entity from its `source_refs` (Fix 1).
/// Each code-symbol entity written by `scan_repo` carries a `SourceLocation
/// .path` on its first evidence ref (the document it was extracted from — see
/// `adapters/ingest/src/extractor.rs`). Returns the first non-empty path, or
/// `None` when the entity has no location-bearing ref (e.g. a manually-written
/// concept entity). Lets `search`/`get_context` surface WHERE code lives
/// without the caller having to read the file.
fn entity_source_path(entity: &KnowledgeEntity) -> Option<String> {
    entity.source_refs.iter().find_map(|r| {
        r.location
            .as_ref()
            .and_then(|l| l.path.clone())
            .filter(|p| !p.is_empty())
    })
}

/// Shorten an `org/name` label to just the repo name (the segment after the
/// last `/`), e.g. `"phanijapps/zbot"` → `"zbot"` (Fix 5 compact format).
/// Returns the input unchanged when there is no `/`.
fn shorten_repo_label(label: &str) -> &str {
    label
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(label)
}

/// True when `name` is an exact/normalized match for `query` (Fix 2). The query
/// equals the name (case-insensitive), or one contains the other after
/// lowercasing + trimming. Handles both `"anthropicOAuth"` → exact, and
/// `"Anthropic OAuth"` → split-match (the name contains the query, or the query
/// contains the name). Short queries (< 3 chars) never match — a 1–2 char
/// needle would inject a flood of false positives.
fn name_matches_query(name: &str, query: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    if q.len() < 3 || n.is_empty() {
        return false;
    }
    n == q || n.contains(&q) || q.contains(&n)
}

/// A resolved `search` hit carrying everything needed for rendering and the
/// diversity/score gates (Fixes 1–5). Built from a recall `RetrievalResult`
/// (the normal path) or an exact-match-injected entity (Fix 2).
#[derive(Clone)]
struct SearchHit {
    entity_id: String,
    name: String,
    kind_label: String,
    /// Full `org/name` provenance repo label (shortened only at render time).
    repo: String,
    /// Source file path from [`entity_source_path`] (Fix 1). `None` for items
    /// whose entity is unresolvable or carries no location.
    path: Option<String>,
    /// Fusion-trace source (`"lexical"`, `"vector"`, …) or `"exact-match"` for
    /// Fix 2 injected hits.
    retriever: String,
    score: f32,
    /// True when the entity name is an exact/normalized match for the query
    /// (Fix 2). Sorts the hit to the top.
    is_exact: bool,
}

impl SearchHit {
    /// Build a hit from a recall item + the entity it resolved to. The `by_id`
    /// lookup is the caller's responsibility (the recall item carries only the
    /// id). When the entity is no longer present (lookup miss), `name` falls
    /// back to the item's content and `kind_label`/`path` are empty/None.
    fn from_recall(item: &RetrievalResult, by_id: &HashMap<String, KnowledgeEntity>) -> Self {
        let repo = provenance_repo_label(&item.provenance.source);
        let retriever = item
            .fusion_trace
            .as_ref()
            .map(|t| t.source.as_str())
            .unwrap_or("?")
            .to_owned();
        match by_id.get(&item.target_id) {
            Some(e) => SearchHit {
                entity_id: item.target_id.clone(),
                name: e.name.clone(),
                kind_label: format!("{:?}", e.kind),
                repo,
                path: entity_source_path(e),
                retriever,
                score: item.score.total,
                is_exact: false,
            },
            None => SearchHit {
                entity_id: item.target_id.clone(),
                name: if item.content.is_empty() {
                    item.target_id.clone()
                } else {
                    item.content.clone()
                },
                kind_label: String::new(),
                repo,
                path: None,
                retriever,
                score: item.score.total,
                is_exact: false,
            },
        }
    }

    /// Build a synthetic hit for an entity that matches the query exactly but
    /// was not surfaced by recall (Fix 2). Score is floored to 1.0 (top-rank);
    /// retriever is labeled `exact-match` so the boost is visible in
    /// diagnostics mode.
    fn from_exact_match(entity: &KnowledgeEntity) -> Self {
        SearchHit {
            entity_id: entity.id.to_string(),
            name: entity.name.clone(),
            kind_label: format!("{:?}", entity.kind),
            repo: provenance_repo_label(&entity.provenance.source),
            path: entity_source_path(entity),
            retriever: "exact-match".to_owned(),
            score: 1.0,
            is_exact: true,
        }
    }
}

/// Boost exact/normalized name matches to the top of the results (Fix 2).
///
/// 1. Mark any existing hit (already in `hits`) whose entity name matches the
///    query as `is_exact` and floor its score to 1.0 so it sorts first.
/// 2. Inject entities from `by_id` that match the query but are NOT already in
///    `hits` (by entity_id), as synthetic `from_exact_match` hits. Injected
///    entities are deduped among themselves by `(repo, name)`.
///
/// Returns the count of newly-injected hits (existing matches that were merely
/// boosted are NOT counted). The injection is a cheap O(entities) scan —
/// `by_id` is already in memory via [`entity_lookup`].
fn inject_exact_matches(
    hits: &mut Vec<SearchHit>,
    by_id: &HashMap<String, KnowledgeEntity>,
    query: &str,
) -> usize {
    let mut seen_ids: HashSet<String> = HashSet::new();
    for h in hits.iter_mut() {
        if name_matches_query(&h.name, query) {
            h.is_exact = true;
            if h.score < 1.0 {
                h.score = 1.0;
            }
        }
        seen_ids.insert(h.entity_id.clone());
    }
    let mut seen_names: HashSet<(String, String)> = HashSet::new();
    for h in hits.iter() {
        seen_names.insert((h.repo.clone(), h.name.clone()));
    }
    let mut injected = 0usize;
    for e in by_id.values() {
        if !name_matches_query(&e.name, query) {
            continue;
        }
        let id = e.id.to_string();
        if seen_ids.contains(&id) {
            continue;
        }
        let hit = SearchHit::from_exact_match(e);
        if !seen_names.insert((hit.repo.clone(), hit.name.clone())) {
            continue;
        }
        seen_ids.insert(id);
        hits.push(hit);
        injected += 1;
    }
    injected
}

/// Keep at most `max_per_file` hits per source file (Fix 3: diversity). Hits
/// are taken in their current order (exact-first, score-desc), so the
/// top-scoring items per file survive. Returns the kept hits (in order) plus a
/// note line per file that was trimmed. Hits without a path pass through
/// uncapped (per the spec: "If paths aren't available, skip this cap").
fn apply_per_file_cap(hits: Vec<SearchHit>, max_per_file: usize) -> (Vec<SearchHit>, Vec<String>) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut dropped_by_file: HashMap<String, usize> = HashMap::new();
    let mut kept = Vec::with_capacity(hits.len());
    for h in hits {
        match h.path.as_deref() {
            Some(p) if !p.is_empty() => {
                let count = counts.entry(p.to_owned()).or_insert(0);
                if *count < max_per_file {
                    *count += 1;
                    kept.push(h);
                } else {
                    *dropped_by_file.entry(p.to_owned()).or_insert(0) += 1;
                }
            }
            _ => kept.push(h),
        }
    }
    // Sort notes by file path for deterministic output.
    let mut notes: Vec<String> = dropped_by_file
        .into_iter()
        .map(|(file, n)| format!("... [{n} results from {file} capped for diversity]"))
        .collect();
    notes.sort();
    (kept, notes)
}

/// Drop hits whose score is below `min_score` (Fix 5: score threshold). Returns
/// the kept hits (in order) + the count dropped, so a note can be appended.
fn apply_score_threshold(hits: Vec<SearchHit>, min_score: f32) -> (Vec<SearchHit>, usize) {
    let mut dropped = 0usize;
    let kept: Vec<SearchHit> = hits
        .into_iter()
        .filter(|h| {
            if h.score >= min_score {
                true
            } else {
                dropped += 1;
                false
            }
        })
        .collect();
    (kept, dropped)
}

/// Render a hit to a single result line (Fix 1 + Fix 5).
///
/// - Diagnostics (`diagnostics = true`): full format with kind + retriever —
///   `name (kind) — org/repo, path [retriever, score=X.XX]`.
/// - Compact (default): `name — repo, path [X.XX]` — kind dropped (redundant
///   for code), repo shortened to the name segment, retriever dropped, score
///   rounded to 2 decimals. ~50% shorter than the diagnostics line.
///
/// The path (Fix 1) is included in both modes when available.
fn render_hit(h: &SearchHit, diagnostics: bool) -> String {
    let path_part = match h.path.as_deref() {
        Some(p) if !p.is_empty() => format!(", {p}"),
        _ => String::new(),
    };
    if diagnostics {
        let kind_part = if h.kind_label.is_empty() {
            String::new()
        } else {
            format!(" ({})", h.kind_label)
        };
        format!(
            "{}{kind_part} — {}{path_part} [{}, score={:.2}]",
            h.name, h.repo, h.retriever, h.score
        )
    } else {
        let repo = shorten_repo_label(&h.repo);
        format!("{} — {}{path_part} [{:.2}]", h.name, repo, h.score)
    }
}

/// Render a one-line discovery header for a recall item (Fix 4): the result
/// metadata WITHOUT the content body. Format:
/// `name (kind) — org/repo, path [X.XX]`. Falls back to a short content
/// snippet when the entity is not resolvable (memories, chunks). ~80 chars vs
/// ~2000 for an evidence excerpt, so a discovery packet costs ~25x less context.
fn discovery_header(item: &RetrievalResult, by_id: &HashMap<String, KnowledgeEntity>) -> String {
    let repo = provenance_repo_label(&item.provenance.source);
    match by_id.get(&item.target_id) {
        Some(e) => {
            let path_part = entity_source_path(e)
                .map(|p| format!(", {p}"))
                .unwrap_or_default();
            format!(
                "{} ({:?}) — {}{path_part} [{:.2}]",
                e.name, e.kind, repo, item.score.total
            )
        }
        None => {
            // Non-entity item (memory/chunk/belief): show a short snippet +
            // target_type so the line is still informative without the body.
            let snippet: String = if item.content.is_empty() {
                item.target_id.clone()
            } else {
                item.content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(80)
                    .collect()
            };
            format!(
                "{snippet} {:?} — {} [{:.2}]",
                item.target_type, repo, item.score.total
            )
        }
    }
}

/// `search`: ranked code-symbol search over indexed entities.
///
/// Routes through the unified (hybrid) recall — lexical (BM25) + graph +
/// associative-graph + community-summary lanes fuse over weighted RRF — so
/// multi-term and natural-language queries (e.g. `"reciprocal rank fusion"`)
/// return ranked symbol hits. The lexical lane resolves entity-id BM25 hits to
/// their code symbol (the resolver is entity-aware), so symbols indexed by
/// `scan_repo` are reachable. Replaces the prior whole-string `.contains()`
/// loop, which missed any query that was not a verbatim substring of
/// `"{name} {kind}"` (the §6.3 defect: `search "reciprocal rank fusion"` →
/// "No results").
///
/// Backward compatible: when recall is unavailable, search degrades to the
/// direct entity-list scan (the old path) rather than erroring.
pub fn search(app: &App, args: &Value) -> Result<Value, ToolError> {
    let query = req_str(args, "query")?;
    // Optional repository filter (e.g. "phanijapps/engram",
    // "github.com/phanijapps/engram"). When present, recall hits and the
    // fallback scan are narrowed to that repository by provenance, eliminating
    // cross-repo contamination in a shared-scope DB. Absent = all repos
    // (current behavior). A blank/empty value is treated as absent.
    let repository = args
        .get("repository")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_owned());
    let limit = args["limit"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(10)
        .clamp(1, 100);

    // Prefer the hybrid-recall path. Each recall item carries the fused rank;
    // entity (code-symbol) hits are kept and rendered as `name (kind)`.
    //
    // Returns the rendered hits PLUS an optional no-results diagnostic (built
    // from the raw recall payload before the Entity filter) so an empty packet
    // is never silent.
    let (result_lines, notes, no_results_diag): (Vec<String>, Vec<String>, Option<String>) =
        match app.provider.require_recall() {
            Ok(handle) => {
                let request = RetrievalRequest {
                    query: query.to_owned(),
                    scope: app.scope.clone(),
                    requester: requester(),
                    modes: Vec::new(),
                    filters: None,
                    cues: Vec::new(),
                    limit: Some(limit as u32),
                    budget: None,
                    include_explanations: Some(true),
                };
                let payload = block_on(handle.recall(request)).map_err(internal)?;
                let by_id = entity_lookup(app);

                // Diagnostics captured BEFORE the Entity filter so an empty
                // result can report how much recall actually found.
                let total_recall = payload.items.len();
                let lanes: HashSet<&str> = payload
                    .items
                    .iter()
                    .filter_map(|i| i.fusion_trace.as_ref().map(|t| t.source.as_str()))
                    .collect();

                // Repository post-filter: when `repository` is set, drop any
                // item whose provenance source is not from that repo. The recall
                // lanes themselves cannot be repo-scoped without a domain-level
                // filter (deferred to a future RFC), so this post-filter is the
                // contamination guard for the search output.
                let mut filtered: Vec<&RetrievalResult> = payload
                    .items
                    .iter()
                    .filter(|i| i.target_type == RetrievalTargetType::Entity)
                    .filter(|i| match repository.as_deref() {
                        Some(repo) => source_matches_repository(&i.provenance.source, repo),
                        None => true,
                    })
                    .collect();

                // Stable source-identity dedup: collapse items that share the
                // same (repo, entity_name, entity_kind), keeping the
                // higher-scoring one. The recall fusion already dedups by
                // (target_type, target_id), but two entities with different IDs
                // but the same name+repo (e.g. entity vs chunk dual
                // representation, or re-scan duplicates) should not both appear.
                filtered.sort_by(|a, b| {
                    b.score
                        .total
                        .partial_cmp(&a.score.total)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut seen: HashSet<(String, String, String)> = HashSet::new();
                let deduped: Vec<&RetrievalResult> = filtered
                    .into_iter()
                    .filter(|i| {
                        let repo = provenance_repo_label(&i.provenance.source);
                        let (name, kind) = match by_id.get(&i.target_id) {
                            Some(e) => (e.name.clone(), format!("{:?}", e.kind)),
                            None => (i.content.clone(), "?".to_owned()),
                        };
                        seen.insert((repo, name, kind))
                    })
                    .collect();
                let entity_hits = deduped.len();

                // Build structured hits (Fix 1: file path resolved here, inside
                // SearchHit::from_recall).
                let mut hits: Vec<SearchHit> = deduped
                    .iter()
                    .map(|i| SearchHit::from_recall(i, &by_id))
                    .collect();

                // Fix 2: exact-match injection — boost/inject entities whose
                // name exactly or split-matches the query, so an identifier
                // match never loses to semantic similarity.
                inject_exact_matches(&mut hits, &by_id, query);

                // Re-sort: exact matches first, then by score descending.
                hits.sort_by(|a, b| {
                    b.is_exact.cmp(&a.is_exact).then_with(|| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                });

                // Fix 5: score threshold — drop barely-relevant noise before
                // the per-file cap so capped noise doesn't consume per-file
                // slots. Exact-match hits are floored to 1.0 above and always
                // survive the default 0.01 floor.
                let min_score = args["min_score"]
                    .as_f64()
                    .map(|v| v as f32)
                    .unwrap_or(DEFAULT_MIN_SCORE);
                let (hits, threshold_dropped) = apply_score_threshold(hits, min_score);

                // Fix 3: per-file diversity cap — keep at most 2 per source file.
                let (hits, file_notes) = apply_per_file_cap(hits, MAX_PER_FILE);

                // Render. Compact is the default (Fix 5); `diagnostics = true`
                // selects the full kind + retriever format.
                let diagnostics = args["diagnostics"].as_bool().unwrap_or(false);
                let result_lines: Vec<String> = hits
                    .iter()
                    .take(limit)
                    .map(|h| render_hit(h, diagnostics))
                    .collect();

                let mut notes: Vec<String> = Vec::new();
                if threshold_dropped > 0 {
                    notes.push(format!(
                        "... [{threshold_dropped} items below score threshold]"
                    ));
                }
                notes.extend(file_notes);

                let diag = if result_lines.is_empty() {
                    Some(format_no_results_diag(
                        total_recall,
                        entity_hits,
                        lanes.len(),
                        &payload.source_failures,
                    ))
                } else {
                    None
                };
                (result_lines, notes, diag)
            }
            Err(_) => {
                // Degrade to a direct scan when recall is not wired (e.g. a
                // capability-check failure) so search still works without hybrid.
                (
                    substring_symbol_scan(app, query, limit, repository.as_deref()),
                    Vec::new(),
                    None,
                )
            }
        };

    let mut body = if result_lines.is_empty() {
        let mut msg = "No results.".to_owned();
        if let Some(diag) = no_results_diag {
            msg.push_str(&format!(" ({diag})"));
        }
        if let Some(repo) = repository.as_deref() {
            msg.push('\n');
            msg.push_str(&repository_empty_note(repo));
        }
        msg
    } else {
        result_lines.join("\n")
    };
    for note in &notes {
        body.push('\n');
        body.push_str(note);
    }
    Ok(protocol::text_content(body))
}

/// Builds an `entity_id → KnowledgeEntity` lookup over the project scope for
/// consistent symbol rendering. Empty when the knowledge-query capability is
/// unavailable (callers fall back to recall item content).
///
/// Prefers the shared graph snapshot cache: a search already populated it via
/// the recall lanes, so re-resolving result entity ids reuses the materialized
/// entities instead of reloading all ~36k of them from the store on every query.
/// Falls back to `list_entities` on a miss (or no cache); it does not populate
/// the cache itself (it reads entities only, no relationships — see the graph
/// lane docs for the same reasoning).
fn entity_lookup(app: &App) -> HashMap<String, KnowledgeEntity> {
    if let Some(cache) = app.provider.graph_cache()
        && let Some(snap) = block_on(cache.get(&app.scope))
    {
        return snap
            .entities
            .iter()
            .map(|e| (e.id.to_string(), e.clone()))
            .collect();
    }
    app.provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_entities(&app.scope)).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.id.to_string(), e))
        .collect()
}

/// Fallback symbol scan (the pre-hybrid path): lists entities and keeps those
/// whose `"{name} {kind}"` contains the query as a substring. Used only when
/// unified recall is unavailable. When `repository` is set, entities are
/// additionally narrowed by provenance so the degrade path does not reintroduce
/// cross-repo contamination.
fn substring_symbol_scan(
    app: &App,
    query: &str,
    limit: usize,
    repository: Option<&str>,
) -> Vec<String> {
    let knowledge_query = match app.provider.require_knowledge_query() {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    let entities = block_on(knowledge_query.list_entities(&app.scope)).unwrap_or_default();
    let needle = query.to_lowercase();
    entities
        .iter()
        .filter(|e| {
            format!("{} {:?}", e.name, e.kind)
                .to_lowercase()
                .contains(&needle)
        })
        .filter(|e| match repository {
            Some(repo) => source_matches_repository(&e.provenance.source, repo),
            None => true,
        })
        .take(limit)
        .map(|e| format!("{} ({:?})", e.name, e.kind))
        .collect()
}

/// `symbol_context`: callers, callees, and community for one symbol.
pub fn symbol_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let symbol = req_str(args, "symbol")?;
    let depth = args["depth"].as_u64().unwrap_or(1) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;
    let ctx = engram_codegraph_queries::symbol_context_bounded(&rels, symbol, depth, cap);
    Ok(protocol::text_content(format!("{ctx:?}")))
}

/// `change_impact`: blast radius + dependency path from a change site.
pub fn change_impact(app: &App, args: &Value) -> Result<Value, ToolError> {
    let target = req_str(args, "target")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;
    let radius = engram_codegraph_queries::blast_radius_bounded(&rels, target, depth, cap);
    let path = args["to"]
        .as_str()
        .and_then(|to| engram_codegraph_queries::dependency_path(&rels, target, to));
    Ok(protocol::text_content(format!(
        "Blast radius ({depth} hops, cap {cap}): {radius:?}\nDependency path: {path:?}"
    )))
}

/// `code_health`: dead code (zero-caller symbols) + repository stats.
pub fn code_health(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let rels = fetch_rels(app)?;
    let dead = engram_codegraph_queries::dead_code(&rels);
    let stats = engram_codegraph_queries::repository_stats(&rels);
    Ok(protocol::text_content(format!(
        "Dead code ({} symbols): {dead:?}\nStats: {stats:?}",
        dead.len()
    )))
}

/// `architecture`: central symbols, bridges, communities, stats — one map.
pub fn architecture(app: &App, args: &Value) -> Result<Value, ToolError> {
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let rels = fetch_rels(app)?;
    let central = engram_codegraph_queries::central_symbols(&rels, limit);
    let bridges = engram_codegraph_queries::bridge_symbols(&rels, limit);
    let communities = engram_codegraph_queries::call_communities(&rels, 3);
    let stats = engram_codegraph_queries::repository_stats(&rels);
    Ok(protocol::text_content(format!(
        "Central: {central:?}\nBridges: {bridges:?}\nCommunities: {communities:?}\nStats: {stats:?}"
    )))
}

/// `whats_changed`: temporal recency + impact + compound + overview.
/// (`directional` deferred — needs scan-baseline retention.)
pub fn whats_changed(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let query = app.provider.require_knowledge_query().map_err(internal)?;
    let entities = block_on(query.list_entities(&app.scope)).unwrap_or_default();
    let rels = fetch_rels(app)?;
    let versions: Vec<engram_codegraph_temporal::VersionedSymbol> = entities
        .iter()
        .map(|e| {
            let name = e.name.as_str();
            engram_codegraph_temporal::VersionedSymbol {
                key: name.to_owned(),
                valid_from: e.valid_from,
                valid_until: e.valid_until,
                in_degree: rels
                    .iter()
                    .filter(|r| r.object.name.as_deref() == Some(name))
                    .count(),
                out_degree: rels
                    .iter()
                    .filter(|r| r.subject.name.as_deref() == Some(name))
                    .count(),
            }
        })
        .collect();
    let now = chrono::Utc::now();
    let recent = engram_codegraph_temporal::recent(&versions, now, 14.0);
    let impact = engram_codegraph_temporal::impact(&versions);
    let compound = engram_codegraph_temporal::compound(&versions, now, 14.0);
    let communities = engram_codegraph_queries::call_communities(&rels, 3);
    let overview = engram_codegraph_temporal::overview(&communities);
    Ok(protocol::text_content(format!(
        "Recent: {recent:?}\nImpact: {impact:?}\nCompound: {compound:?}\nOverview: {overview:?}"
    )))
}

// --- Phase 3 -----------------------------------------------------------------

/// The shape of a `get_context` focus, used to set per-lane budgets. A
/// code-shaped question should surface code symbols; a doc/concept question
/// should surface durable memory + text. See [`classify_query_shape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryShape {
    Code,
    Mixed,
    Doc,
}

impl QueryShape {
    /// Human-readable label for the output header (`query shape: Code`).
    fn label(self) -> &'static str {
        match self {
            QueryShape::Code => "Code",
            QueryShape::Mixed => "Mixed",
            QueryShape::Doc => "Doc",
        }
    }

    /// Per-bucket budget percentages in order `(entity, chunk, memory, other)`.
    /// These are independent caps (not a partition): each only bounds its own
    /// lane, so the sum may drift above or below `limit`.
    fn budgets(self) -> (u32, u32, u32, u32) {
        match self {
            // Code-shaped: code symbols dominate; durable memory held back.
            QueryShape::Code => (60, 25, 10, 5),
            // Neutral split.
            QueryShape::Mixed => (35, 35, 20, 10),
            // Doc-shaped: durable memory + text dominate; symbols are sparse.
            QueryShape::Doc => (15, 40, 35, 10),
        }
    }
}

/// Coarse target-type bucket for lane-budget accounting. `Entity` (code
/// symbols), `Chunk` (code/doc text), and `Memory` (durable memory) are
/// first-class lanes; everything else (belief, relationship, concept, …) falls
/// into `Other`, which carries the smallest budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TargetBucket {
    Entity,
    Chunk,
    Memory,
    Other,
}

impl TargetBucket {
    /// Lowercase label for the per-lane cap note (`2 memory items capped`).
    fn label(self) -> &'static str {
        match self {
            TargetBucket::Entity => "entity",
            TargetBucket::Chunk => "chunk",
            TargetBucket::Memory => "memory",
            TargetBucket::Other => "other",
        }
    }
}

/// Map a [`RetrievalTargetType`] into its budget lane.
fn target_bucket(tt: &RetrievalTargetType) -> TargetBucket {
    match tt {
        RetrievalTargetType::Entity => TargetBucket::Entity,
        RetrievalTargetType::Chunk => TargetBucket::Chunk,
        RetrievalTargetType::Memory => TargetBucket::Memory,
        _ => TargetBucket::Other,
    }
}

/// Classify a `get_context` focus as [`QueryShape::Code`], [`QueryShape::Mixed`],
/// or [`QueryShape::Doc`] by counting how many code-shape signals it carries.
///
/// Signals (each is a code indicator; the COUNT sets the shape):
/// 1. camelCase / PascalCase identifiers — `[a-z][A-Z]` or `[A-Z][a-z]+[A-Z]`.
/// 2. ALL_CAPS snake_case constants — `[A-Z]{2,}_`.
/// 3. Source-tree markers — file extensions (`.rs`, `.ts`, …) or path segments
///    (`/src/`, `crates/`, …).
/// 4. Code keywords as whole words — `fn`, `function`, `class`, `struct`,
///    `def`, `impl`, `enum`, `trait`, `interface`, `method`, `return`, `async`,
///    `import`.
/// 5. Backtick-quoted identifiers.
///
/// 0 signals → `Doc`; 1–2 → `Mixed`; 3+ → `Code`.
fn classify_query_shape(focus: &str) -> QueryShape {
    use std::sync::OnceLock;
    static CASE_RE: OnceLock<regex::Regex> = OnceLock::new();
    static CONST_RE: OnceLock<regex::Regex> = OnceLock::new();
    static PATH_RE: OnceLock<regex::Regex> = OnceLock::new();
    static KEYWORD_RE: OnceLock<regex::Regex> = OnceLock::new();
    static BACKTICK_RE: OnceLock<regex::Regex> = OnceLock::new();

    let case = CASE_RE.get_or_init(|| {
        regex::Regex::new(r"[a-z][A-Z]|[A-Z][a-z]+[A-Z]").expect("query-shape case regex")
    });
    let const_re =
        CONST_RE.get_or_init(|| regex::Regex::new(r"[A-Z]{2,}_").expect("query-shape const regex"));
    let path = PATH_RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\.(rs|ts|tsx|jsx|py|go|java|kt|cpp|cc|h|hpp|rb|js|swift|scala)\b|(?:^|[/\\])(?:src|crates|core|adapters|bindings|packages|mcp|lib)(?:[/\\]|$)",
        )
        .expect("query-shape path regex")
    });
    let keyword = KEYWORD_RE.get_or_init(|| {
        regex::Regex::new(
            r"\b(fn|function|class|struct|def|impl|enum|trait|interface|method|return|async|import)\b",
        )
        .expect("query-shape keyword regex")
    });
    let backtick = BACKTICK_RE
        .get_or_init(|| regex::Regex::new(r"`[^`]+`").expect("query-shape backtick regex"));

    let mut signals = 0;
    if case.is_match(focus) {
        signals += 1;
    }
    if const_re.is_match(focus) {
        signals += 1;
    }
    if path.is_match(focus) {
        signals += 1;
    }
    if keyword.is_match(focus) {
        signals += 1;
    }
    if backtick.is_match(focus) {
        signals += 1;
    }

    match signals {
        0 => QueryShape::Doc,
        1 | 2 => QueryShape::Mixed,
        _ => QueryShape::Code,
    }
}

/// Apply per-target-type lane budgets to a rank-ordered recall slice.
///
/// Items are taken in RRF rank order, but each [`TargetBucket`] is capped at its
/// budget share of `limit`; over-cap items (the lowest-ranked of the over-budget
/// type) are dropped. Returns the kept items (still in rank order) plus a
/// per-bucket note for each lane that was trimmed.
///
/// The RRF fusion itself stays equal-weight — this is an output-assembly cap,
/// not a re-weighting. The caller passes the slice AFTER any repository filter,
/// so budgets apply to the already-filtered set.
fn apply_lane_budgets<'a>(
    items: &[&'a RetrievalResult],
    shape: QueryShape,
    limit: usize,
) -> (Vec<&'a RetrievalResult>, Vec<String>) {
    let (e_pct, c_pct, m_pct, o_pct) = shape.budgets();
    // Floor of `limit * pct / 100`, with a minimum of 1 for any non-zero share
    // so a small `limit` never zeroes a lane entirely (a single belief still
    // survives a code query when room allows).
    let cap_for = |pct: u32| -> usize {
        let cap = limit * pct as usize / 100;
        if pct > 0 { cap.max(1) } else { 0 }
    };
    let caps = [
        (TargetBucket::Entity, cap_for(e_pct)),
        (TargetBucket::Chunk, cap_for(c_pct)),
        (TargetBucket::Memory, cap_for(m_pct)),
        (TargetBucket::Other, cap_for(o_pct)),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let mut counts: HashMap<TargetBucket, usize> = HashMap::new();
    let mut dropped: HashMap<TargetBucket, usize> = HashMap::new();
    let mut kept: Vec<&'a RetrievalResult> = Vec::with_capacity(items.len());
    for item in items {
        let bucket = target_bucket(&item.target_type);
        let cap = caps[&bucket];
        let count = counts.entry(bucket).or_insert(0);
        if *count < cap {
            *count += 1;
            kept.push(*item);
        } else {
            *dropped.entry(bucket).or_insert(0) += 1;
        }
    }

    // One note per lane that was trimmed, in a stable bucket order.
    let notes = [
        TargetBucket::Entity,
        TargetBucket::Chunk,
        TargetBucket::Memory,
        TargetBucket::Other,
    ]
    .into_iter()
    .filter_map(|b| {
        dropped
            .get(&b)
            .map(|n| format!("\n... [{n} {} items capped by lane budget]", b.label()))
    })
    .collect();
    (kept, notes)
}

/// `get_context`: compose a task-aware context packet for a focus (symbol, file,
/// concept, or free-text). Fuses recall (docs + memories + beliefs) with the
/// code neighborhood (callers/callees/community) — a pragmatic first version of
/// RFC-0013's `ContextSubgraph` packet, built on existing capabilities.
pub fn get_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let focus = req_str(args, "focus")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 50) as u32;
    // Optional repository filter — see `search`. Narrows the [Recall] section
    // (post-filter on items) AND the [Code]/[Graph] sections (pre-filter on the
    // graph edge set) so a query about one repo never returns another repo's
    // symbols, docs, or edges. Absent = all repos.
    let repository = args
        .get("repository")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_owned());

    // Classify the query shape (Code / Mixed / Doc) from the focus so per-lane
    // budgets can prioritize code evidence for code-shaped queries. Computed
    // once here — used both to budget the [Recall] lanes and to label the
    // output header (`query shape: Code`).
    let shape = classify_query_shape(focus);

    // Fix 4: discovery vs evidence mode. `discovery` (alias: `compact`) returns
    // ONLY result headers (name, kind, repo, path, score) — no content body —
    // so a discovery packet costs ~80 chars/item vs ~2000 for evidence. Default
    // is `evidence` (the prior behavior: content excerpts up to
    // CONTEXT_ITEM_EXCERPT_CHARS).
    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("evidence");
    let discovery = mode == "discovery" || mode == "compact";

    // Built once and reused for both the [Recall] rendering (discovery headers
    // need entity name/kind/path) and the anchor derivation below — avoids the
    // double `entity_lookup` store read the prior code did.
    let by_id = entity_lookup(app);

    // 1. Fused recall (docs + memories + beliefs matching the focus). The
    //    payload is retained alongside the rendered text so the top-scoring
    //    Entity hit can drive the code/graph anchor (step 2/3) — a NL focus
    //    never matches a symbol name exactly, so without that anchor the
    //    structural sections would always be empty for NL queries.
    let (recall_text, recall_items) = match app.provider.require_recall() {
        Ok(handle) => {
            let req = RetrievalRequest {
                query: focus.to_owned(),
                scope: app.scope.clone(),
                requester: crate::tools::requester(),
                modes: Vec::new(),
                filters: None,
                cues: Vec::new(),
                limit: Some(limit),
                budget: None,
                include_explanations: Some(true),
            };
            let mut payload = block_on(handle.recall(req)).map_err(internal)?;
            // Repository post-filter on the recall items. This narrows BOTH the
            // rendered [Recall] section and the anchor derivation (step 1b) to
            // the target repo. Recall lanes themselves stay unscoped (a
            // domain-level filter is deferred), so this is the contamination
            // guard for the [Recall] output.
            if let Some(repo) = repository.as_deref() {
                payload
                    .items
                    .retain(|i| source_matches_repository(&i.provenance.source, repo));
            }
            // Cap both per-item SIZE (a class-level chunk can be thousands of
            // lines) and TOTAL assembled size so `get_context` cannot bloat a
            // downstream prompt by tens of thousands of tokens. The `limit`
            // arg still bounds item COUNT; these caps bound character SIZE.
            //
            // Lane budgets (GC-6): AFTER the repo filter, truncate to `limit`
            // (defensive — recall should already respect it), then cap how many
            // of each `RetrievalTargetType` survive into [Recall]. This keeps
            // durable memory + docs from crowding out code evidence on a
            // code-shaped query. The RRF fusion stays equal-weight; this is an
            // output-assembly cap only.
            let ranked: Vec<&engram_domain::RetrievalResult> =
                payload.items.iter().take(limit as usize).collect();
            let (taken, cap_notes) = apply_lane_budgets(&ranked, shape, limit as usize);
            let total = taken.len();
            let mut joined = String::new();
            let mut added = 0usize;
            if discovery {
                // Fix 4: headers only — `name (kind) — repo, path [score]`,
                // one line per item, no content body.
                for i in &taken {
                    let header = discovery_header(i, &by_id);
                    let sep = if joined.is_empty() { "" } else { "\n" };
                    if joined.len() + sep.len() + header.len() > CONTEXT_TOTAL_CHAR_BUDGET {
                        break;
                    }
                    joined.push_str(sep);
                    joined.push_str(&header);
                    added += 1;
                }
            } else {
                for i in &taken {
                    let item_excerpt = excerpt(&i.content, CONTEXT_ITEM_EXCERPT_CHARS);
                    // Per-item provenance suffix so a caller can see which retriever
                    // produced each recall item + its fused score, mirroring `search`.
                    let prov = recall_provenance_suffix(i);
                    let sep = if joined.is_empty() { "" } else { "\n---\n" };
                    if joined.len() + sep.len() + item_excerpt.len() + prov.len()
                        > CONTEXT_TOTAL_CHAR_BUDGET
                    {
                        // This item would blow the budget; stop adding items.
                        break;
                    }
                    joined.push_str(sep);
                    joined.push_str(&item_excerpt);
                    joined.push_str(&prov);
                    added += 1;
                }
            }
            let omitted = total - added;
            if omitted > 0 {
                joined.push_str(&format!(
                    "\n... [budget reached, {omitted} more items omitted]"
                ));
            }
            // Per-lane cap notes: one line per bucket trimmed by the lane
            // budgets (distinct from the character-budget note above).
            for note in &cap_notes {
                joined.push_str(note);
            }
            (joined, payload.items)
        }
        Err(_) => (String::new(), Vec::new()),
    };

    // 1b. Derive an anchor symbol: the name of the top-scoring Entity recall
    //     hit (recall items are fused-ranked, so the first Entity item is the
    //     best symbol match for the focus). The NL focus still drives recall;
    //     this anchor only drives the structural [Code]/[Graph] sections, which
    //     need an exact symbol name. Falls back to the raw focus when recall
    //     found no entity. When a repository filter is active the items are
    //     already repo-narrowed, so the anchor resolves to a target-repo symbol.
    let anchor_symbol = recall_items
        .iter()
        .find(|i| i.target_type == RetrievalTargetType::Entity)
        .and_then(|i| by_id.get(&i.target_id).map(|e| e.name.clone()))
        .unwrap_or_else(|| focus.to_owned());

    // 2. Code neighborhood keyed on the anchor symbol. When a repository filter
    //    is active the edge set is pre-filtered to that repo. Degrade on error.
    let (rels, graph_status) = match graph_relationships_filtered(app, repository.as_deref()) {
        Ok(r) => (r, String::new()),
        Err(e) => (Vec::new(), format!("(graph unavailable: {})", e.message)),
    };
    let code_ctx = engram_codegraph_queries::symbol_context(&rels, &anchor_symbol, depth);

    // 3. Unified-graph links — doc/concept `describes`/`mentions` edges for the
    //    anchor symbol, on top of the code neighborhood. This is the doc↔code
    //    connection surfacing in one context packet. `rels` is already
    //    repo-filtered when a filter is set, so cross-repo edges are excluded.
    let links: Vec<String> = rels
        .iter()
        .filter_map(|r| {
            let (s, o) = (r.subject.name.as_deref()?, r.object.name.as_deref()?);
            if s == anchor_symbol {
                Some(format!("{anchor_symbol} -[{}]-> {o}", r.predicate))
            } else if o == anchor_symbol {
                Some(format!("{s} -[{}]-> {anchor_symbol}", r.predicate))
            } else {
                None
            }
        })
        .take(20)
        .collect();
    let graph_text = if !graph_status.is_empty() {
        graph_status
    } else if links.is_empty() {
        "(none)".to_owned()
    } else {
        links.join("\n")
    };

    // Surface the anchor resolution so a caller can see which symbol the
    // structural sections resolved to (empty for an exact-symbol focus that
    // also has no matching entity, since anchor == focus in that case).
    let anchor_note = if anchor_symbol == focus {
        String::new()
    } else {
        format!(" (anchor symbol: {anchor_symbol})")
    };

    // Surface the detected query shape so a caller can see why the [Recall]
    // lanes were budgeted the way they were.
    let shape_note = format!(" (query shape: {})", shape.label());

    // When a repository filter is active and it removed EVERY result (no
    // recall text and no graph links), surface a diagnostic instead of a
    // silently empty packet. `[Code]` derives from the same filtered `rels` +
    // anchor, so empty recall + empty links implies an empty code section too.
    let repo_note = match (
        repository.as_deref(),
        recall_text.is_empty() && links.is_empty(),
    ) {
        (Some(repo), true) => format!("\n\n{}", repository_empty_note(repo)),
        _ => String::new(),
    };

    Ok(protocol::text_content(format!(
        "=== Context for '{focus}'{anchor_note}{shape_note} ===\n\n[Recall]\n{recall_text}\n\n[Graph]\n{graph_text}\n\n[Code]\n{code_ctx:?}{repo_note}"
    )))
}

/// `capability_report`: report which provider capabilities are wired.
pub fn capability_report(app: &App, _args: &Value) -> Result<Value, ToolError> {
    let caps: Vec<String> = [
        ("memory", app.provider.memory().is_some()),
        ("knowledge", app.provider.knowledge().is_some()),
        ("graph", app.provider.graph().is_some()),
        ("knowledge_query", app.provider.knowledge_query().is_some()),
        ("lexical_feed", app.provider.lexical_feed().is_some()),
        ("recall", app.provider.recall().is_some()),
        ("consolidation", app.provider.consolidation().is_some()),
        ("batch", app.provider.batch().is_some()),
        ("ontology", app.provider.ontology().is_some()),
        ("taxonomy", app.provider.taxonomy().is_some()),
        ("beliefs", app.provider.beliefs().is_some()),
        ("procedures", app.provider.procedures().is_some()),
        ("hierarchy", app.provider.hierarchy().is_some()),
        ("identity", app.provider.identity().is_some()),
        (
            "vector",
            app.provider.embedding_provider().is_some() && app.provider.require_vectors().is_ok(),
        ),
    ]
    .into_iter()
    .map(|(name, ok)| format!("  {name}: {}", if ok { "supported" } else { "unsupported" }))
    .collect();
    Ok(protocol::text_content(format!(
        "Server: engram-mcp 0.1.0\nCapabilities:\n{}",
        caps.join("\n")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("engram-scan-cfg-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".engram")).unwrap();
        root
    }

    /// `api` is 3 chars → builtin rejects it. An allowlist entry forces it on,
    /// which is the observable signal that a config was applied.
    const ALLOW_API_CFG: &str = r#"{ "concepts": { "allowlist": ["api"] } }"#;

    #[test]
    fn resolve_uses_explicit_scan_config_arg() {
        let root = tmp_root("explicit");
        let cfg_path = root.join("my-scan.json");
        std::fs::write(&cfg_path, ALLOW_API_CFG).unwrap();

        let args = json!({ "scan_config": cfg_path.to_string_lossy() });
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(filter.should_link_concept("api"), "allowlist applied");
        assert!(note.contains("scan_config applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_discovers_repo_local_config() {
        let root = tmp_root("discovered");
        // No `scan_config` arg, but <root>/.engram/scan.json exists.
        std::fs::write(root.join(".engram").join("scan.json"), ALLOW_API_CFG).unwrap();

        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(
            filter.should_link_concept("api"),
            "discovered config applied"
        );
        assert!(note.contains(".engram/scan.json applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_falls_back_to_builtin_when_absent() {
        let root = tmp_root("builtin");
        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        // Builtin rejects short names like "api".
        assert!(!filter.should_link_concept("api"));
        assert!(note.contains("builtin filter"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_soft_fails_on_malformed_config() {
        let root = tmp_root("malformed");
        std::fs::write(root.join(".engram").join("scan.json"), "{ not json").unwrap();

        let args = json!({});
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        // Soft-fail → builtin (rejects "api"), note explains the ignore.
        assert!(!filter.should_link_concept("api"));
        assert!(note.contains("ignored"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn normalize_repository_key_strips_host_prefixes() {
        // org/name passes through, lowercased + trimmed.
        assert_eq!(
            normalize_repository_key("phanijapps/engram"),
            "phanijapps/engram"
        );
        assert_eq!(
            normalize_repository_key("  PhanijApps/Engram "),
            "phanijapps/engram"
        );
        assert_eq!(
            normalize_repository_key("earendil-works/pi"),
            "earendil-works/pi"
        );

        // Fully-qualified spellings collapse to the org/name core regardless of
        // separator, so the SSH-remote form inside provenance.source matches.
        assert_eq!(
            normalize_repository_key("github.com/phanijapps/engram"),
            "phanijapps/engram"
        );
        assert_eq!(
            normalize_repository_key("github.com:phanijapps/engram"),
            "phanijapps/engram"
        );
        assert_eq!(
            normalize_repository_key("git@github.com:phanijapps/engram"),
            "phanijapps/engram"
        );

        // A bare host (no org/name) is degenerate — it must NOT match every
        // GitHub-sourced row.
        assert_eq!(normalize_repository_key("github.com"), "");
        assert_eq!(normalize_repository_key(""), "");
        assert_eq!(normalize_repository_key("   "), "");
    }

    #[test]
    fn source_matches_repository_against_scan_provenance() {
        // The exact bracketed form `scan_repo` writes into provenance.source.
        let src = "engram-mcp-scan [git@github.com:phanijapps/engram.git@feat/x:abc123]";
        // All three caller spellings of the same repo must match.
        assert!(source_matches_repository(src, "phanijapps/engram"));
        assert!(source_matches_repository(
            src,
            "github.com/phanijapps/engram"
        ));
        assert!(source_matches_repository(
            src,
            "git@github.com:phanijapps/engram"
        ));
        assert!(source_matches_repository(src, "PhanijApps/Engram"));

        // A different repo does not match.
        assert!(!source_matches_repository(src, "phanijapps/zbot"));
        assert!(!source_matches_repository(src, "earendil-works/pi"));

        // Degenerate filters match nothing (no false positives).
        assert!(!source_matches_repository(src, "github.com"));
        assert!(!source_matches_repository(src, ""));

        // A non-github remote form is also handled (substring after the
        // org/name core), so a self-hosted repo spelled as org/name matches.
        let other = "engram-mcp-scan [git@git.internal.corp:earendil-works/pi.git@main:dead]";
        assert!(source_matches_repository(other, "earendil-works/pi"));
        assert!(!source_matches_repository(other, "phanijapps/engram"));
    }

    #[test]
    fn resolve_explicit_arg_overrides_discovered() {
        // Both an explicit arg AND a discovered file exist; the arg wins and the
        // discovered file's content must NOT leak into the filter. The
        // discovered file is given a VALID sentinel (an allowlist entry that
        // would force-link "api" if it leaked) so any leak fails the test —
        // using a broken discovered file would not prove read-order priority.
        let root = tmp_root("override");
        std::fs::write(
            root.join(".engram").join("scan.json"),
            r#"{ "concepts": { "allowlist": ["api"] } }"#,
        )
        .unwrap();
        let cfg_path = root.join("explicit.json");
        // Blocklist a name the builtin accepts, to prove the explicit file loaded.
        std::fs::write(
            &cfg_path,
            r#"{ "concepts": { "blocklist": ["RetrievalIndex"] } }"#,
        )
        .unwrap();

        let args = json!({ "scan_config": cfg_path.to_string_lossy() });
        let (filter, note) = resolve_scan_filter(root.to_str().unwrap(), &args);

        assert!(
            !filter.should_link_concept("RetrievalIndex"),
            "explicit blocklist applied"
        );
        assert!(
            !filter.should_link_concept("api"),
            "discovered allowlist must not leak (api stays builtin-rejected)"
        );
        assert!(note.contains("scan_config applied"), "note: {note}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn provenance_repo_label_extracts_org_repo_from_git_remote() {
        // SSH-remote form embedded by scan_repo.
        let ssh = "engram-mcp-scan [git@github.com:phanijapps/engram.git@feat/x:abc123]";
        assert_eq!(provenance_repo_label(ssh), "phanijapps/engram");

        // HTTPS form.
        let https = "engram-mcp-scan [https://github.com/phanijapps/engram.git@main:dead]";
        assert_eq!(provenance_repo_label(https), "phanijapps/engram");

        // A different repo.
        let other = "engram-mcp-scan [git@github.com:earendil-works/pi.git@main:cafe]";
        assert_eq!(provenance_repo_label(other), "earendil-works/pi");
    }

    #[test]
    fn provenance_repo_label_falls_back_to_source_name_without_remote() {
        // No bracketed remote → bare source name.
        assert_eq!(provenance_repo_label("engram-mcp-scan"), "engram-mcp-scan");
        // Empty string.
        assert_eq!(provenance_repo_label(""), "");
    }

    #[test]
    fn format_no_results_diag_reports_counts_and_errors() {
        use engram_domain::{RetrievalMode, RetrievalSourceFailure, SourceFailureSeverity};
        // No failures: just counts.
        let diag = format_no_results_diag(15, 3, 4, &[]);
        assert!(diag.contains("recall returned 15 items"), "{diag}");
        assert!(diag.contains("3 entity hits"), "{diag}");
        assert!(diag.contains("4 lanes contributed"), "{diag}");
        assert!(!diag.contains("errored"), "{diag}");

        // With a failed lane.
        let failure = RetrievalSourceFailure {
            source: "lexical".to_owned(),
            mode: Some(RetrievalMode::Keyword),
            severity: SourceFailureSeverity::Warning,
            reason: "source_error".to_owned(),
            message: None,
            degraded: true,
        };
        let diag = format_no_results_diag(0, 0, 0, &[failure]);
        assert!(diag.contains("errored: lexical (source_error)"), "{diag}");
    }

    // --- query-shape classification + lane budgets (GC-6) -------------------

    /// Build a minimal `RetrievalResult` for lane-budget tests. Only
    /// `target_type` and the id vary; everything else is fixture filler (the
    /// budget logic reads `target_type` only).
    fn recall_item(tt: RetrievalTargetType, id: &str) -> RetrievalResult {
        use engram_domain::{
            Actor, ActorKind, AllowedUse, Id, Policy, Provenance, Retention, RetrievalScore,
            Visibility,
        };
        RetrievalResult {
            id: id.to_owned(),
            target_type: tt,
            target_id: id.to_owned(),
            content: format!("content-{id}"),
            score: RetrievalScore {
                total: 1.0,
                relevance: None,
                recency: None,
                confidence: None,
                cue_match: None,
                hierarchical_fit: None,
                policy_fit: None,
            },
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("tester"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            explanation: None,
            fusion_trace: None,
            metadata: None,
        }
    }

    #[test]
    fn classify_doc_focus_when_no_code_signals() {
        // No camelCase, no ALL_CAPS, no path, no keyword, no backticks → Doc.
        assert_eq!(
            classify_query_shape("What are the project principles?"),
            QueryShape::Doc
        );
        assert_eq!(
            classify_query_shape("summarize the design philosophy"),
            QueryShape::Doc
        );
    }

    #[test]
    fn classify_mixed_focus_with_one_or_two_signals() {
        // camelCase (vaultExplorer) + keyword (import) = 2 signals → Mixed.
        assert_eq!(
            classify_query_shape("How does vaultExplorer handle the import?"),
            QueryShape::Mixed
        );
        // A single keyword alone = 1 signal → Mixed.
        assert_eq!(
            classify_query_shape("Explain the function of this layer"),
            QueryShape::Mixed
        );
    }

    #[test]
    fn classify_code_focus_with_three_or_more_signals() {
        // path (.rs) + camelCase (vaultExplorer) + keyword (struct) + backtick
        // = 4 signals → Code.
        assert_eq!(
            classify_query_shape(
                "In codegraph.rs, how does vaultExplorer struct work? See `RetrievalResult`"
            ),
            QueryShape::Code
        );
        // ALL_CAPS constant + keyword (struct) + file path (.rs) = 3 signals → Code.
        assert_eq!(
            classify_query_shape("Where is MAX_NEIGHBORHOOD_CAP set on the struct in src/lib.rs?"),
            QueryShape::Code
        );
    }

    #[test]
    fn lane_budget_caps_memory_on_code_query() {
        // limit = 24, Code shape → memory cap = floor(24 * 10 / 100) = 2.
        // Five memory items ranked ahead of an entity → only the top 2 memory
        // items survive; the rest are dropped with a cap note. The entity is
        // unaffected (entity cap = 14).
        let items = vec![
            recall_item(RetrievalTargetType::Memory, "m1"),
            recall_item(RetrievalTargetType::Memory, "m2"),
            recall_item(RetrievalTargetType::Memory, "m3"),
            recall_item(RetrievalTargetType::Memory, "m4"),
            recall_item(RetrievalTargetType::Memory, "m5"),
            recall_item(RetrievalTargetType::Entity, "e1"),
        ];
        let refs: Vec<&RetrievalResult> = items.iter().collect();
        let (kept, notes) = apply_lane_budgets(&refs, QueryShape::Code, 24);

        // Highest-ranked memory kept first, then the entity; rank order preserved.
        let kept_ids: Vec<&str> = kept.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(kept_ids, vec!["m1", "m2", "e1"]);

        // Exactly one cap note, for the memory lane, reporting 3 dropped.
        assert_eq!(notes.len(), 1, "notes: {notes:?}");
        assert!(
            notes[0].contains("3 memory items capped by lane budget"),
            "{}",
            notes[0]
        );
    }

    #[test]
    fn lane_budget_gives_memory_more_room_on_doc_query() {
        // limit = 20, Doc shape → memory cap = floor(20 * 35 / 100) = 7.
        // Five memory items all survive under the Doc budget (under Code they
        // would cap at 2). No lanes trimmed → no notes.
        let items = vec![
            recall_item(RetrievalTargetType::Memory, "m1"),
            recall_item(RetrievalTargetType::Memory, "m2"),
            recall_item(RetrievalTargetType::Memory, "m3"),
            recall_item(RetrievalTargetType::Memory, "m4"),
            recall_item(RetrievalTargetType::Memory, "m5"),
        ];
        let refs: Vec<&RetrievalResult> = items.iter().collect();
        let (kept, notes) = apply_lane_budgets(&refs, QueryShape::Doc, 20);
        assert_eq!(kept.len(), 5, "all memory survives under Doc budget");
        assert!(notes.is_empty(), "no lanes capped: {notes:?}");
    }

    #[test]
    fn lane_budget_caps_other_lane_and_emits_note() {
        // limit = 10, Mixed → other cap = max(floor(10 * 10 / 100), 1) = 1.
        // Two beliefs (Other bucket) → one kept, one dropped, note for 'other'.
        let items = vec![
            recall_item(RetrievalTargetType::Belief, "b1"),
            recall_item(RetrievalTargetType::Belief, "b2"),
        ];
        let refs: Vec<&RetrievalResult> = items.iter().collect();
        let (kept, notes) = apply_lane_budgets(&refs, QueryShape::Mixed, 10);
        assert_eq!(kept.len(), 1);
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].contains("1 other items capped by lane budget"),
            "{}",
            notes[0]
        );
    }

    // --- accurate + compact retrieval (Fixes 1–5) -----------------------------

    /// Build a minimal `KnowledgeEntity` for the path/exact-match/cap tests.
    /// `path` populates the first `source_ref.location.path` (Fix 1). The
    /// `provenance.source` is set to a scan-style string so `provenance_repo_label`
    /// resolves an org/name.
    fn make_entity(
        id: &str,
        name: &str,
        kind: engram_domain::EntityKind,
        path: Option<&str>,
    ) -> KnowledgeEntity {
        use engram_domain::{
            Actor, ActorKind, EvidenceRef, EvidenceTargetType, Id, SourceLocation,
        };
        let source_refs = vec![EvidenceRef {
            target_type: EvidenceTargetType::Document,
            target_id: None,
            uri: None,
            quote: None,
            location: path.map(|p| SourceLocation {
                path: Some(p.to_owned()),
                start_line: None,
                end_line: None,
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
        }];
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope: crate::scope::project_scope("test-project", "default"),
            source_refs,
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: engram_domain::Provenance {
                source: format!("engram-mcp-scan [git@github.com:phanijapps/zbot.git@main:abc]"),
                actor: Actor {
                    id: Id::from("tester"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: None,
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        }
    }

    /// Build a `SearchHit` with the fields the gate/render tests read. The other
    /// fields default to empty / `false`.
    fn hit(name: &str, path: Option<&str>, score: f32) -> SearchHit {
        SearchHit {
            entity_id: format!("id-{name}"),
            name: name.to_owned(),
            kind_label: "Function".to_owned(),
            repo: "phanijapps/zbot".to_owned(),
            path: path.map(|p| p.to_owned()),
            retriever: "lexical".to_owned(),
            score,
            is_exact: false,
        }
    }

    #[test]
    fn entity_source_path_resolves_from_source_refs() {
        // Entity with a path on its first source_ref → path returned.
        let e = make_entity(
            "e1",
            "alpha",
            engram_domain::EntityKind::Function,
            Some("src/lib.rs"),
        );
        assert_eq!(entity_source_path(&e).as_deref(), Some("src/lib.rs"));

        // Entity with no path on the ref → None.
        let e_no_path = make_entity("e2", "beta", engram_domain::EntityKind::Struct, None);
        assert!(entity_source_path(&e_no_path).is_none());
    }

    #[test]
    fn name_matches_query_exact_case_insensitive_and_split() {
        // Exact, case-insensitive.
        assert!(name_matches_query("anthropicOAuth", "anthropicOAuth"));
        assert!(name_matches_query("anthropicOAuth", "AnthropicOAuth"));

        // Query contains the name (split-match: "Anthropic OAuth" contains "oauth").
        assert!(name_matches_query("oauth", "Anthropic OAuth handler"));

        // Name contains the query.
        assert!(name_matches_query("loginAnthropic", "login"));

        // Too-short query never matches (avoids false-positive flood).
        assert!(!name_matches_query("loginAnthropic", "fn"));
        assert!(!name_matches_query("loginAnthropic", ""));

        // Unrelated name does not match.
        assert!(!name_matches_query(
            "reciprocalRankFusion",
            "anthropicOAuth"
        ));
    }

    #[test]
    fn shorten_repo_label_drops_org_prefix() {
        assert_eq!(shorten_repo_label("phanijapps/zbot"), "zbot");
        assert_eq!(shorten_repo_label("earendil-works/pi"), "pi");
        // No slash → unchanged.
        assert_eq!(shorten_repo_label("local"), "local");
    }

    #[test]
    fn inject_exact_matches_boosts_existing_and_injects_missing() {
        // by_id has alpha (already in hits) + anthropicOAuth (not in hits) + gamma (unrelated).
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        by_id.insert(
            "e1".into(),
            make_entity(
                "e1",
                "alpha",
                engram_domain::EntityKind::Function,
                Some("a.rs"),
            ),
        );
        by_id.insert(
            "e2".into(),
            make_entity(
                "e2",
                "anthropicOAuth",
                engram_domain::EntityKind::Function,
                Some("c.rs"),
            ),
        );
        by_id.insert(
            "e3".into(),
            make_entity(
                "e3",
                "gamma",
                engram_domain::EntityKind::Struct,
                Some("d.rs"),
            ),
        );

        // hits has one recall item: alpha (entity_id e1, low score).
        let mut hits = vec![SearchHit {
            entity_id: "e1".into(),
            name: "alpha".into(),
            kind_label: "Function".into(),
            repo: "phanijapps/zbot".into(),
            path: Some("a.rs".into()),
            retriever: "lexical".into(),
            score: 0.05,
            is_exact: false,
        }];

        // Query "alpha": alpha is already present → boosted (is_exact + score 1.0),
        // NOT re-injected. anthropicOAuth does not match "alpha".
        let injected = inject_exact_matches(&mut hits, &by_id, "alpha");
        assert_eq!(injected, 0, "alpha already present → not re-injected");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].is_exact, "alpha boosted to exact");
        assert!(
            (hits[0].score - 1.0).abs() < 1e-6,
            "alpha score floored to 1.0"
        );

        // Query "anthropicOAuth": not in hits → injected as a new synthetic hit.
        let injected = inject_exact_matches(&mut hits, &by_id, "anthropicOAuth");
        assert_eq!(injected, 1, "anthropicOAuth injected");
        // The injected hit is at the end (caller re-sorts).
        let inj = hits.last().unwrap();
        assert_eq!(inj.name, "anthropicOAuth");
        assert!(inj.is_exact);
        assert!((inj.score - 1.0).abs() < 1e-6);
        assert_eq!(inj.retriever, "exact-match");
        assert_eq!(inj.path.as_deref(), Some("c.rs"));

        // gamma never matched either query → never injected.
        assert!(hits.iter().all(|h| h.name != "gamma"));
    }

    #[test]
    fn apply_per_file_cap_keeps_two_per_file_and_notes_rest() {
        // Three hits from a.rs, one from b.rs, one path-less (passthrough).
        let hits = vec![
            hit("m1", Some("a.rs"), 0.9),
            hit("m2", Some("a.rs"), 0.8),
            hit("m3", Some("a.rs"), 0.7),
            hit("m4", Some("b.rs"), 0.6),
            hit("m5", None, 0.5),
        ];
        let (kept, notes) = apply_per_file_cap(hits, 2);
        // 2 from a.rs + 1 from b.rs + 1 path-less = 4 kept.
        assert_eq!(kept.len(), 4);
        // The top-2 from a.rs survive (m1, m2); m3 dropped.
        let names: Vec<&str> = kept.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"m1"));
        assert!(names.contains(&"m2"));
        assert!(!names.contains(&"m3"), "m3 should be capped");
        assert!(names.contains(&"m4"));
        assert!(
            names.contains(&"m5"),
            "path-less hit passes through uncapped"
        );
        // One note, for a.rs, reporting 1 dropped.
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].contains("1 results from a.rs capped for diversity"),
            "{}",
            notes[0]
        );
    }

    #[test]
    fn apply_score_threshold_drops_low_score_and_counts() {
        let hits = vec![
            hit("keep1", Some("a.rs"), 0.5),
            hit("drop", Some("b.rs"), 0.005),
            hit("keep2", Some("c.rs"), 0.02),
        ];
        let (kept, dropped) = apply_score_threshold(hits, 0.01);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped, 1);
        let names: Vec<&str> = kept.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"keep1"));
        assert!(names.contains(&"keep2"));
        assert!(!names.contains(&"drop"));
    }

    #[test]
    fn render_hit_compact_drops_kind_and_shortens_repo() {
        let h = hit(
            "loginAnthropic",
            Some("packages/ai/src/auth/oauth/anthropic.ts"),
            0.0382,
        );
        // Compact: name — repo_name, path [score]  (no kind, no retriever).
        let compact = render_hit(&h, false);
        assert!(
            compact.starts_with("loginAnthropic — zbot, packages/ai/src/auth/oauth/anthropic.ts"),
            "compact format: {compact}"
        );
        assert!(
            compact.contains("[0.04]"),
            "score rounded to 2 decimals: {compact}"
        );
        assert!(
            !compact.contains("Function"),
            "kind dropped in compact: {compact}"
        );
        assert!(
            !compact.contains("lexical"),
            "retriever dropped in compact: {compact}"
        );

        // Diagnostics: full format with kind + retriever + score.
        let diag = render_hit(&h, true);
        assert!(
            diag.starts_with("loginAnthropic (Function) — phanijapps/zbot, packages/ai/src/auth/oauth/anthropic.ts"),
            "diagnostics format: {diag}"
        );
        assert!(
            diag.contains("[lexical, score=0.04]"),
            "full provenance: {diag}"
        );
    }

    #[test]
    fn render_hit_omits_path_gracefully_when_absent() {
        let h = hit("noPath", None, 0.5);
        let compact = render_hit(&h, false);
        // No ", path" segment — just "name — repo [score]".
        assert_eq!(compact, "noPath — zbot [0.50]");
    }

    #[test]
    fn discovery_header_formats_name_kind_repo_path_score() {
        // Entity resolvable → "name (kind) — repo, path [score]".
        // (`recall_item` sets provenance.source = "test", so the repo label is
        // "test" — the test asserts the FORMAT, not a specific remote.)
        let item = recall_item(RetrievalTargetType::Entity, "e1");
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        by_id.insert(
            "e1".into(),
            make_entity(
                "e1",
                "alpha",
                engram_domain::EntityKind::Function,
                Some("src/lib.rs"),
            ),
        );
        let header = discovery_header(&item, &by_id);
        assert!(
            header.starts_with("alpha (Function) — test, src/lib.rs"),
            "discovery header: {header}"
        );
        assert!(header.contains("[1.00]"), "score included: {header}");
        // Discovery mode must NOT include the content body.
        assert!(
            !header.contains("content-e1"),
            "no content in discovery: {header}"
        );
    }

    #[test]
    fn discovery_header_falls_back_to_snippet_for_non_entity() {
        // A Memory item whose target_id is not in by_id → snippet + target_type.
        let item = recall_item(RetrievalTargetType::Memory, "m1");
        let by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        let header = discovery_header(&item, &by_id);
        assert!(
            header.contains("Memory"),
            "target_type shown for non-entity: {header}"
        );
        assert!(
            header.contains("content-m1"),
            "content snippet shown as label: {header}"
        );
    }
}
