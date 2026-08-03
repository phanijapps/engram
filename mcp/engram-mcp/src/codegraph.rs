//! Code-intelligence tools (RFC-0015 Phase 2 + 3): `scan_repo` + the composites
//! + `search` + `get_context`.
//!
//! `scan_repo` uses a fan-in adapter so treesitter ingestion routes through
//! the provider's handles (no engine-store bypass).

use engram_domain::{
    KnowledgeChunk, KnowledgeEntity, KnowledgeRelationship, RetrievalRequest, RetrievalResult,
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
use crate::tools::{internal, invalid, policy, req_str, requester, system_actor};

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

/// Excerpt length for chunk text surfaced in `search` results. Enough to see
/// decisive code values (`sk-ant-oat`, `Authorization: Bearer`, `PKCE`) that
/// live in function bodies, not entity names. Bounds context cost while
/// surfacing the code TEXT the agent actually needs.
const CHUNK_EXCERPT_CHARS: usize = 500;

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

/// Renders a provenance bracket for one recall item, surfacing BOTH the raw
/// per-lane score and the fused RRF score (Option B): `[lane, raw=X.XX, rrf=Y.YY]`.
/// When the lane did not set a raw score, falls back to `[lane, rrf=Y.YY]`. Used
/// as a per-item suffix in `get_context` evidence output so a caller can see
/// WHERE each result came from AND how strongly that lane matched on its own
/// scale (cosine / BM25 / …), not just the fused rank score.
fn recall_provenance_suffix(item: &RetrievalResult) -> String {
    match item.fusion_trace.as_ref() {
        Some(t) => match t.source_score {
            Some(raw) => format!(
                "[{}, raw={:.2}, rrf={:.2}]",
                t.source, raw, item.score.total
            ),
            None => format!("[{}, rrf={:.2}]", t.source, item.score.total),
        },
        None => format!("[?, rrf={:.2}]", item.score.total),
    }
}

/// Builds the concise diagnostics appended after "No results." when recall
/// returned zero usable hits (Entity or Chunk). Surfaces (1) how many raw items
/// recall produced before the target-type filter, (2) how many hits survived
/// (after dedup), (3) how many lanes contributed, and (4) any lane that errored.
/// This turns a silent empty packet into an actionable signal.
fn format_no_results_diag(
    total_recall: usize,
    deduped_hits: usize,
    lanes: usize,
    failures: &[RetrievalSourceFailure],
) -> String {
    let mut parts = vec![
        format!("recall returned {total_recall} items"),
        format!("{deduped_hits} hits after filter+dedup"),
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

/// Build a short label for a chunk's content: the first non-empty line,
/// truncated to 60 chars. Used as the `name` in a chunk [`SearchHit`] so the
/// header line is informative without dumping the whole chunk text (which can
/// be thousands of chars). Returns `"(empty chunk)"` for whitespace-only
/// content so the hit still renders something.
fn chunk_label(content: &str) -> String {
    let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed = first_line.trim();
    let label: String = trimmed.chars().take(60).collect();
    if label.is_empty() {
        "(empty chunk)".to_owned()
    } else {
        label
    }
}

/// Build a `chunk_id → KnowledgeChunk` lookup over the project scope, used to
/// resolve the source file path for chunk recall items (the `RetrievalResult`
/// carries only `target_id` + `content`, not the path). Empty when the
/// knowledge-query capability is unavailable. Built lazily by `search` ONLY
/// when chunk recall items are present (entity-only searches skip this).
fn chunk_lookup(app: &App) -> HashMap<String, KnowledgeChunk> {
    app.provider
        .require_knowledge_query()
        .ok()
        .and_then(|q| block_on(q.list_chunks(&app.scope)).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.id.to_string(), c))
        .collect()
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
    // Guard: query too short, name too short/empty → no match.
    if q.len() < 3 || n.len() < 3 || n.is_empty() {
        return false;
    }
    // Exact: query IS the identifier.
    if n == q {
        return true;
    }
    // Name contains the query (short query → longer identifier).
    // e.g., query "login" → name "loginAnthropic".
    if n.contains(&q) {
        return true;
    }
    // Query contains the name — ONLY for short (≤2 token) queries.
    // For multi-token NL queries, this would match any common word
    // ("request", "model", "log") that appears in the query → false positives.
    let q_tokens = q.split_whitespace().count();
    if q_tokens <= 2 && q.contains(&n) {
        return true;
    }
    false
}

/// True when `name` is an EXACT (case-insensitive, trimmed) match for `query`
/// — the query IS the identifier, nothing more. Used by `inject_exact_matches`
/// (Option B) to give a true identifier match a higher injected score than a
/// substring/partial match. Distinct from [`name_matches_query`], which also
/// accepts substring containment.
fn name_exact_match(name: &str, query: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    !q.is_empty() && n == q
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
    /// Raw per-lane score from `fusion_trace.source_score` (Option B): the best
    /// contributor's pre-fusion score (cosine / BM25 / …) — how strongly the
    /// lane matched on its own scale. `None` for lanes that didn't set one and
    /// for synthetic exact-match hits (no lane produced them).
    source_score: Option<f32>,
    /// The fused RRF score (`item.score.total`). Drives ranking + the score
    /// threshold; surfaced alongside `source_score` so an agent can tell a
    /// strong raw match from a weak one without the artificial 1.0 floor.
    score: f32,
    /// True when the entity name is an exact/normalized match for the query
    /// (Fix 2). Used for the `[exact-match]` render tag + sort priority. It is
    /// NOT used for score manipulation (Option B removed the 1.0 boost).
    is_exact: bool,
    /// For Chunk results: a bounded excerpt (first [`CHUNK_EXCERPT_CHARS`] chars)
    /// of the chunk text, rendered as a second line after the header. `None`
    /// for Entity hits (the normal case). When `Some`, this hit is a Chunk and
    /// the excerpt surfaces the code TEXT (function bodies, credential strings,
    /// request construction) that filesystem grep would find but entity-name
    /// search cannot.
    chunk_excerpt: Option<String>,
}

impl SearchHit {
    /// Build a hit from a recall item + the entity it resolved to. The `by_id`
    /// lookup is the caller's responsibility (the recall item carries only the
    /// id). When the entity is no longer present (lookup miss), `name` falls
    /// back to the item's content and `kind_label`/`path` are empty/None.
    fn from_recall(item: &RetrievalResult, by_id: &HashMap<String, KnowledgeEntity>) -> Self {
        let repo = provenance_repo_label(&item.provenance.source);
        let trace = item.fusion_trace.as_ref();
        let retriever = trace.map(|t| t.source.as_str()).unwrap_or("?").to_owned();
        // Option B: surface the raw per-lane score alongside the fused RRF.
        let source_score = trace.and_then(|t| t.source_score);
        match by_id.get(&item.target_id) {
            Some(e) => SearchHit {
                entity_id: item.target_id.clone(),
                name: e.name.clone(),
                kind_label: format!("{:?}", e.kind),
                repo,
                path: entity_source_path(e),
                retriever,
                source_score,
                score: item.score.total,
                is_exact: false,
                chunk_excerpt: None,
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
                source_score,
                score: item.score.total,
                is_exact: false,
                chunk_excerpt: None,
            },
        }
    }

    /// Build a synthetic hit for an entity that matches the query but was not
    /// surfaced by recall (Fix 2). Option B: the score is ANCHOR-based (caller
    /// passes the max RRF score for exact matches, or 80% of it for partial
    /// matches) — NOT an artificial 1.0. `source_score` is `None` (no lane
    /// produced the hit). `retriever` is labeled `exact-match` so the injection
    /// is visible in diagnostics mode and the `[exact-match]` render tag fires.
    fn from_exact_match(entity: &KnowledgeEntity, score: f32) -> Self {
        SearchHit {
            entity_id: entity.id.to_string(),
            name: entity.name.clone(),
            kind_label: format!("{:?}", entity.kind),
            repo: provenance_repo_label(&entity.provenance.source),
            path: entity_source_path(entity),
            retriever: "exact-match".to_owned(),
            source_score: None,
            score,
            is_exact: true,
            chunk_excerpt: None,
        }
    }

    /// Build a hit for a Chunk recall item. Chunks carry the code TEXT (function
    /// bodies, credential strings, request construction), not symbol names — so
    /// the `name` is a short label derived from the first non-empty line
    /// ([`chunk_label`]), and the full bounded excerpt (first
    /// [`CHUNK_EXCERPT_CHARS`] chars) is carried in `chunk_excerpt` for rendering
    /// as a second line. The path comes from the chunk's `SourceLocation` when
    /// available (looked up from the store via [`chunk_lookup`]).
    fn from_recall_chunk(item: &RetrievalResult, chunk_path: Option<&str>) -> Self {
        let repo = provenance_repo_label(&item.provenance.source);
        let trace = item.fusion_trace.as_ref();
        let retriever = trace.map(|t| t.source.as_str()).unwrap_or("?").to_owned();
        let source_score = trace.and_then(|t| t.source_score);
        let name = chunk_label(&item.content);
        let excerpt_text = excerpt(&item.content, CHUNK_EXCERPT_CHARS);
        SearchHit {
            entity_id: item.target_id.clone(),
            name,
            kind_label: "Chunk".to_owned(),
            repo,
            path: chunk_path.map(|p| p.to_owned()),
            retriever,
            source_score,
            score: item.score.total,
            is_exact: false,
            chunk_excerpt: Some(excerpt_text),
        }
    }
}

/// Mark + inject exact/normalized name matches (Fix 2, Option B: no score boost).
///
/// 1. Mark any existing hit (already in `hits`) whose entity name matches the
///    query as `is_exact`. Its RRF score is LEFT UNCHANGED — the raw lane score
///    (now surfaced in rendering) shows whether it's a strong match, so the
///    artificial 1.0 floor is gone. `is_exact` still sorts it first and tags it
///    `[exact-match]` in diagnostics.
/// 2. Inject entities from `by_id` that match the query but are NOT already in
///    `hits` (by entity_id), as synthetic `from_exact_match` hits. Because these
///    weren't in recall (no RRF score), they get an ANCHOR-based score derived
///    from the existing hits' max RRF:
///      - exact identifier match (query == name):     max_rrf        (top rank)
///      - substring / partial match:                  0.8 * max_rrf
///
///    This ranks them realistically instead of flooding the packet with 1.0s.
///    Injected entities are deduped among themselves by `(repo, name)`.
///
/// Returns the count of newly-injected hits (existing matches that were merely
/// marked are NOT counted). The injection is a cheap O(entities) scan —
/// `by_id` is already in memory via [`entity_lookup`].
fn inject_exact_matches(
    hits: &mut Vec<SearchHit>,
    by_id: &HashMap<String, KnowledgeEntity>,
    query: &str,
) -> usize {
    // (1) Mark existing matches; do NOT touch their scores (Option B). Chunks
    // are skipped — `is_exact` means "identifier match," and a chunk's `name`
    // is a content label, not a symbol name.
    let mut seen_ids: HashSet<String> = HashSet::new();
    for h in hits.iter_mut() {
        if h.chunk_excerpt.is_none() && name_matches_query(&h.name, query) {
            h.is_exact = true;
        }
        seen_ids.insert(h.entity_id.clone());
    }

    // Anchor for injected matches: the max RRF score among existing hits, so an
    // injected exact identifier ranks at the top with a realistic score. When
    // there are no hits to anchor against (recall returned nothing for this
    // query), fall back to a small constant above DEFAULT_MIN_SCORE so the
    // injected match still clears the score threshold rather than being dropped.
    let max_rrf = hits.iter().map(|h| h.score).fold(0.0_f32, f32::max);
    let anchor = if max_rrf > 0.0 {
        max_rrf
    } else {
        DEFAULT_MIN_SCORE * 5.0 // 0.05 — above the 0.01 score floor
    };

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
        // Score by match quality (Option B): exact identifier → anchor (top),
        // substring/partial → 80% of anchor.
        let score = if name_exact_match(&e.name, query) {
            anchor
        } else {
            anchor * 0.8
        };
        let hit = SearchHit::from_exact_match(e, score);
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

/// Build the score bracket for a hit, surfacing the raw per-lane score, the
/// fused RRF score, AND the within-query normalized score (Fix 4: score
/// calibration). The normalized score is `raw_rrf / max_rrf_in_result_set`, so
/// the top result renders as ~1.00 and the rest scale proportionally — giving
/// the agent meaningful separation (1.00 vs 0.80 vs 0.30) without changing the
/// underlying fusion. Four shapes:
///
/// - Injected exact-match (`retriever == "exact-match"`, no raw score):
///   `[exact-match, rrf=X.XX (norm:Y.YY)]`.
/// - Lane set a raw score:
///   - compact:     `[lane:raw, rrf=X.XX (norm:Y.YY)]`
///   - diagnostics: `[lane, raw=X.XX, rrf=X.XX (norm:Y.YY)]`
/// - Lane did not set a raw score (None):
///   - compact:     `[rrf=X.XX (norm:Y.YY)]`
///   - diagnostics: `[lane, rrf=X.XX (norm:Y.YY)]`  (lane still named)
///
/// `norm_divisor` is the max RRF in the result set; the caller computes it once
/// over the final hit list and passes it to every `render_hit` call. A
/// non-positive divisor (empty result set) yields `norm:0.00` rather than NaN.
fn score_bracket(h: &SearchHit, diagnostics: bool, norm_divisor: f32) -> String {
    let norm = if norm_divisor > 0.0 {
        h.score / norm_divisor
    } else {
        0.0
    };
    if h.retriever == "exact-match" {
        return format!("[exact-match, rrf={:.2} (norm:{norm:.2})]", h.score);
    }
    match (h.source_score, diagnostics) {
        (Some(raw), false) => format!(
            "[{}:{raw:.2}, rrf={:.2} (norm:{norm:.2})]",
            h.retriever, h.score
        ),
        (Some(raw), true) => format!(
            "[{}, raw={raw:.2}, rrf={:.2} (norm:{norm:.2})]",
            h.retriever, h.score
        ),
        (None, false) => format!("[rrf={:.2} (norm:{norm:.2})]", h.score),
        (None, true) => format!("[{}, rrf={:.2} (norm:{norm:.2})]", h.retriever, h.score),
    }
}

/// Render a hit to a single result line (Fix 1 + Fix 4 + Fix 5 + Option B raw
/// scores).
///
/// - Diagnostics (`diagnostics = true`): full format with kind + retriever —
///   `name (kind) — org/repo, path [lane, raw=X.XX, rrf=Y.YY (norm:Z.ZZ)]`.
/// - Compact (default): `name — repo, path [lane:raw, rrf=Y.YY (norm:Z.ZZ)]` —
///   kind dropped (redundant for code), repo shortened to the name segment,
///   score rounded to 2 decimals. ~50% shorter than the diagnostics line.
///
/// `norm_divisor` (Fix 4) is the max RRF across the result set; it produces the
/// within-query normalized score `raw_rrf / max` so the agent sees rank
/// separation (top = 1.00, others scale down) alongside the raw lane score.
///
/// The path (Fix 1) is included in both modes when available.
///
/// For Chunk hits (`chunk_excerpt` is `Some`): the header line is followed by a
/// second line containing the bounded excerpt (first CHUNK_EXCERPT_CHARS chars
/// of the chunk text). This surfaces the decisive code VALUES (credential
/// strings, constants, request construction) that live in function bodies, not
/// symbol names — the key addition over Entity-only search.
fn render_hit(h: &SearchHit, diagnostics: bool, norm_divisor: f32) -> String {
    let path_part = match h.path.as_deref() {
        Some(p) if !p.is_empty() => format!(", {p}"),
        _ => String::new(),
    };
    let bracket = score_bracket(h, diagnostics, norm_divisor);
    let header = if diagnostics {
        let kind_part = if h.kind_label.is_empty() {
            String::new()
        } else {
            format!(" ({})", h.kind_label)
        };
        format!("{}{kind_part} — {}{path_part} {bracket}", h.name, h.repo)
    } else {
        let repo = shorten_repo_label(&h.repo);
        format!("{} — {}{path_part} {bracket}", h.name, repo)
    };
    // For Chunk hits, append the content excerpt after the header line.
    match &h.chunk_excerpt {
        Some(excerpt_text) => format!("{header}\n{excerpt_text}"),
        None => header,
    }
}

/// Render a one-line discovery header for a recall item (Fix 4 + Option B): the
/// result metadata WITHOUT the content body, surfacing the raw per-lane score
/// alongside the fused RRF score. Format:
/// `name (kind) — org/repo, path [lane, raw=X.XX, rrf=Y.YY]`. Falls back to a
/// short content snippet when the entity is not resolvable (memories, chunks).
/// ~80 chars vs ~2000 for an evidence excerpt, so a discovery packet costs
/// ~25x less context.
fn discovery_header(item: &RetrievalResult, by_id: &HashMap<String, KnowledgeEntity>) -> String {
    let repo = provenance_repo_label(&item.provenance.source);
    let trace = item.fusion_trace.as_ref();
    let lane = trace.map(|t| t.source.as_str()).unwrap_or("?").to_owned();
    let raw = trace.and_then(|t| t.source_score);
    let rrf = item.score.total;
    let bracket = match raw {
        Some(r) => format!("[{lane}, raw={r:.2}, rrf={rrf:.2}]"),
        None => format!("[{lane}, rrf={rrf:.2}]"),
    };
    match by_id.get(&item.target_id) {
        Some(e) => {
            let path_part = entity_source_path(e)
                .map(|p| format!(", {p}"))
                .unwrap_or_default();
            format!("{} ({:?}) — {}{path_part} {bracket}", e.name, e.kind, repo)
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
            format!("{snippet} {:?} — {} {bracket}", item.target_type, repo)
        }
    }
}

/// True when a recall item's target type should be kept in `search` results,
/// given the `include_chunks` setting. Entity (code-symbol) items always pass;
/// Chunk (code-text) items pass only when `include_chunks` is true (the
/// default); all other target types (Memory, Belief, …) are dropped — `search`
/// is a code search, and those lanes are served by `recall` / `get_context`.
fn should_keep_target_type(tt: &RetrievalTargetType, include_chunks: bool) -> bool {
    match tt {
        RetrievalTargetType::Entity => true,
        RetrievalTargetType::Chunk => include_chunks,
        _ => false,
    }
}

/// `search`: ranked code-symbol + code-text search over indexed entities and
/// chunks.
///
/// Routes through the unified (hybrid) recall — lexical (BM25) + graph +
/// associative-graph + community-summary lanes fuse over weighted RRF — so
/// multi-term and natural-language queries (e.g. `"reciprocal rank fusion"`)
/// return ranked symbol hits. The lexical lane resolves entity-id BM25 hits to
/// their code symbol (the resolver is entity-aware), so symbols indexed by
/// `scan_repo` are reachable.
///
/// By default (`include_chunks: true`), the results include BOTH Entity (code
/// symbol) hits AND Chunk (code text) hits. Chunk hits surface the decisive
/// code VALUES (`sk-ant-oat`, `Authorization: Bearer`, `PKCE`) that live in
/// function bodies, not entity names — each chunk hit includes a bounded
/// 500-char excerpt. Set `include_chunks: false` for Entity-only results.
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

    // include_chunks (default true): when true, keep BOTH Entity (symbol) AND
    // Chunk (code text) recall items. Chunks surface the decisive code VALUES
    // (`sk-ant-oat`, `Authorization: Bearer`, `PKCE`) that live in function
    // bodies, not entity names — each chunk hit includes a bounded excerpt.
    // When false, only Entity results are returned (the prior behavior).
    let include_chunks = args["include_chunks"].as_bool().unwrap_or(true);

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

                // Repository post-filter + target-type filter. When
                // `include_chunks` is true, keep BOTH Entity and Chunk items;
                // when false, Entity-only (the prior behavior). The repository
                // filter narrows to the target repo by provenance so a
                // cross-repo query never returns another repo's items.
                let mut filtered: Vec<&RetrievalResult> = payload
                    .items
                    .iter()
                    .filter(|i| should_keep_target_type(&i.target_type, include_chunks))
                    .filter(|i| match repository.as_deref() {
                        Some(repo) => source_matches_repository(&i.provenance.source, repo),
                        None => true,
                    })
                    .collect();

                // Stable source-identity dedup: collapse items that share the
                // same (repo, name, kind), keeping the higher-scoring one. The
                // recall fusion already dedups by (target_type, target_id), but
                // two entities with different IDs but the same name+repo (e.g.
                // entity vs chunk dual representation, or re-scan duplicates)
                // should not both appear. For chunks, the name is the short
                // [`chunk_label`] (not the full text) so the dedup key stays
                // small.
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
                        let (name, kind) = if i.target_type == RetrievalTargetType::Chunk {
                            (chunk_label(&i.content), "Chunk".to_owned())
                        } else {
                            match by_id.get(&i.target_id) {
                                Some(e) => (e.name.clone(), format!("{:?}", e.kind)),
                                None => (i.content.clone(), "?".to_owned()),
                            }
                        };
                        seen.insert((repo, name, kind))
                    })
                    .collect();
                let deduped_hits = deduped.len();

                // Build chunk lookup lazily — only when chunk items survived
                // the filter+dedup. Entity-only searches skip the store read.
                let chunks_by_id = if deduped
                    .iter()
                    .any(|i| i.target_type == RetrievalTargetType::Chunk)
                {
                    chunk_lookup(app)
                } else {
                    HashMap::new()
                };

                // Build structured hits. Entity items use `from_recall` (Fix 1:
                // file path resolved from source_refs); Chunk items use
                // `from_recall_chunk` (bounded text excerpt + path from the
                // chunk's SourceLocation).
                let mut hits: Vec<SearchHit> = deduped
                    .iter()
                    .map(|i| {
                        if i.target_type == RetrievalTargetType::Chunk {
                            let path = chunks_by_id
                                .get(&i.target_id)
                                .and_then(|c| c.location.as_ref())
                                .and_then(|l| l.path.clone())
                                .filter(|p| !p.is_empty());
                            SearchHit::from_recall_chunk(i, path.as_deref())
                        } else {
                            SearchHit::from_recall(i, &by_id)
                        }
                    })
                    .collect();

                // Fix 2: exact-match injection — mark/inject entities whose
                // name exactly or split-matches the query, so an identifier
                // match never loses to semantic similarity. Option B: NO score
                // boost — existing matches keep their RRF score, injected ones
                // get an anchor-based score (max RRF for exact, 80% for partial).
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
                // slots. Option B: exact matches are no longer floored to 1.0,
                // so a low-ranked exact match CAN be dropped here — that's
                // correct (if recall ranked it low, the raw score will say why).
                let min_score = args["min_score"]
                    .as_f64()
                    .map(|v| v as f32)
                    .unwrap_or(DEFAULT_MIN_SCORE);
                let (hits, threshold_dropped) = apply_score_threshold(hits, min_score);

                // Fix 3: per-file diversity cap — keep at most 2 per source file.
                let (hits, file_notes) = apply_per_file_cap(hits, MAX_PER_FILE);

                // Fix 4: score calibration — normalize RRF within the result
                // set so the top result renders as ~1.00 and the rest scale
                // proportionally. The divisor is the max RRF over the FINAL
                // (post-cap, post-threshold) hit list, which is what the agent
                // actually sees. A single-hit set yields norm 1.00 for that
                // hit; an empty set (handled by the max>0 guard in
                // `score_bracket`) yields 0.00 rather than NaN.
                let max_rrf = hits.iter().map(|h| h.score).fold(0.0_f32, f32::max);

                // Render. Compact is the default (Fix 5); `diagnostics = true`
                // selects the full kind + retriever format.
                let diagnostics = args["diagnostics"].as_bool().unwrap_or(false);
                let result_lines: Vec<String> = hits
                    .iter()
                    .take(limit)
                    .map(|h| render_hit(h, diagnostics, max_rrf))
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
                        deduped_hits,
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

/// Parse `args[key]` as EITHER a single string OR a JSON array of strings,
/// returning a `Vec<String>`. This is the batch-call enabler (Fix 1 + Fix 2):
/// `symbol_context` / `change_impact` / `get_context` all accept the legacy
/// single-string form AND the new array form so an agent can resolve N anchors
/// in one call instead of N.
///
/// Rejects:
/// - missing key or null (`{key} is required`)
/// - non-string / non-array value (`{key} is required`)
/// - empty string or empty array (`{key} is required`)
/// - array containing a non-string or empty-string element (`{key} array must
///   contain only non-empty strings`)
///
/// On success, always returns a non-empty `Vec<String>` (length ≥ 1).
fn parse_str_or_array(args: &Value, key: &str) -> Result<Vec<String>, ToolError> {
    match &args[key] {
        Value::String(s) if !s.is_empty() => Ok(vec![s.clone()]),
        Value::Array(arr) => {
            if arr.is_empty() {
                return Err(invalid(format!("{key} is required")));
            }
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                let s = v.as_str().filter(|s| !s.is_empty()).ok_or_else(|| {
                    invalid(format!("{key} array must contain only non-empty strings"))
                })?;
                out.push(s.to_owned());
            }
            Ok(out)
        }
        _ => Err(invalid(format!("{key} is required"))),
    }
}

/// `symbol_context`: callers, callees, and community for one symbol — or, when
/// `symbol` is passed as a JSON array, for each symbol in one call (Fix 1:
/// batch symbol_context). An agent that finds 5 distinctive identifiers makes
/// one call instead of five.
///
/// - String `symbol` (legacy): returns the single `SymbolContextBounded` debug
///   view, unchanged from prior behavior.
/// - Array `symbol` (new): runs `symbol_context_bounded` for each + returns the
///   results concatenated, one section per symbol with a header:
///   `=== symbol_context: <name> (depth=N) ===`.
pub fn symbol_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    let symbols = parse_str_or_array(args, "symbol")?;
    let depth = args["depth"].as_u64().unwrap_or(1) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;

    // Legacy single-string path: identical output to the prior implementation
    // (`{ctx:?}` with no header) so existing callers and tests are unaffected.
    if symbols.len() == 1 && args["symbol"].is_string() {
        let symbol = symbols[0].as_str();
        let ctx = engram_codegraph_queries::symbol_context_bounded(&rels, symbol, depth, cap);
        return Ok(protocol::text_content(format!("{ctx:?}")));
    }

    // Batch path: one section per symbol with a header. The relationship set
    // (`rels`) is fetched ONCE and reused across all symbols — the network/store
    // cost is the same as a single call.
    let mut sections = Vec::with_capacity(symbols.len());
    for symbol in &symbols {
        let ctx = engram_codegraph_queries::symbol_context_bounded(&rels, symbol, depth, cap);
        sections.push(format!(
            "=== symbol_context: {symbol} (depth={depth}) ===\n{ctx:?}"
        ));
    }
    Ok(protocol::text_content(sections.join("\n\n")))
}

/// `change_impact`: blast radius + dependency path from a change site — or, when
/// `target` is passed as a JSON array, for each target in one call (Fix 1:
/// batch change_impact). Mirrors [`symbol_context`]'s batch shape.
///
/// - String `target` (legacy): single blast radius + (optional) dependency path
///   to `to`. Identical to prior output.
/// - Array `target` (new): blast radius per target with a header
///   `=== change_impact: <name> (depth=N) ===`. The `to` / dependency-path
///   option is single-target-only and ignored in batch mode (a per-target `to`
///   would need a parallel array shape that does not exist yet).
pub fn change_impact(app: &App, args: &Value) -> Result<Value, ToolError> {
    let targets = parse_str_or_array(args, "target")?;
    let depth = args["depth"].as_u64().unwrap_or(2) as usize;
    let cap = args["cap"]
        .as_u64()
        .unwrap_or(DEFAULT_NEIGHBORHOOD_CAP as u64) as usize;
    let rels = fetch_rels(app)?;

    // Legacy single-string path: preserve the dependency-path option.
    if targets.len() == 1 && args["target"].is_string() {
        let target = targets[0].as_str();
        let radius = engram_codegraph_queries::blast_radius_bounded(&rels, target, depth, cap);
        let path = args["to"]
            .as_str()
            .and_then(|to| engram_codegraph_queries::dependency_path(&rels, target, to));
        return Ok(protocol::text_content(format!(
            "Blast radius ({depth} hops, cap {cap}): {radius:?}\nDependency path: {path:?}"
        )));
    }

    // Batch path: blast radius per target with a header.
    let mut sections = Vec::with_capacity(targets.len());
    for target in &targets {
        let radius = engram_codegraph_queries::blast_radius_bounded(&rels, target, depth, cap);
        sections.push(format!(
            "=== change_impact: {target} (depth={depth}, cap {cap}) ===\nBlast radius: {radius:?}"
        ));
    }
    Ok(protocol::text_content(sections.join("\n\n")))
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
///
/// **Fix 2 — multi-anchor focus.** `focus` accepts EITHER a single string
/// (legacy) OR a JSON array of strings. When an array is passed:
/// - The FIRST item is the primary anchor for the [Code]/[Graph] sections
///   (current single-anchor behavior, just explicit).
/// - The recall query is the items joined space-separated, so recall searches
///   for ALL terms in one fused pass.
/// - The output header carries `(anchors: [sym1, sym2, sym3])`.
/// This lets an agent pass `focus: ["loginAnthropic", "resolveStoredOAuth",
/// "createClient"]` in one call instead of 3 separate get_context calls.
///
/// **Fix 3 — escalation handoff.** The packet ends with an `=== Assessment ===`
/// section: item counts by target type, graph/code neighborhood status, the
/// resolved anchor, missing-evidence notes (when [Code] or [Graph] is empty),
/// and suggested next-step calls referencing the actual focus terms + anchor.
pub fn get_context(app: &App, args: &Value) -> Result<Value, ToolError> {
    // Fix 2: focus may be a string (legacy) or an array of anchors. The FIRST
    // array item is the primary anchor; the joined string drives recall + the
    // shape classifier. For a single string, primary == joined == focus.
    let focus_list = parse_str_or_array(args, "focus")?;
    let focus_array_passed = args["focus"].is_array();
    let primary_focus = focus_list[0].clone();
    let focus = focus_list.join(" ");

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

    // Classify the query shape (Code / Mixed / Doc) from the joined focus so
    // per-lane budgets can prioritize code evidence for code-shaped queries.
    // Computed once here — used both to budget the [Recall] lanes and to label
    // the output header (`query shape: Code`).
    let shape = classify_query_shape(&focus);

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
    //
    //    The recall query is the JOINED focus (all anchors space-separated) so
    //    a multi-anchor packet searches for every term in one fused pass. For
    //    a single string focus, joined == the focus (unchanged behavior).
    let (recall_text, recall_items) = match app.provider.require_recall() {
        Ok(handle) => {
            let req = RetrievalRequest {
                query: focus.clone(),
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

    // Fix 3: capture recall counts by target type (over the full recall payload,
    // after the repo filter) for the Assessment section's missing-evidence
    // detection. These reflect what RECALL found, which drives the "missing
    // evidence" notes (a capped-empty display still counts as recall-found).
    let recall_entity_count = recall_items
        .iter()
        .filter(|i| i.target_type == RetrievalTargetType::Entity)
        .count();
    let recall_chunk_count = recall_items
        .iter()
        .filter(|i| i.target_type == RetrievalTargetType::Chunk)
        .count();
    let recall_memory_count = recall_items
        .iter()
        .filter(|i| i.target_type == RetrievalTargetType::Memory)
        .count();

    // 1b. Derive an anchor symbol: the name of the top-scoring Entity recall
    //     hit (recall items are fused-ranked, so the first Entity item is the
    //     best symbol match for the focus). The NL focus still drives recall;
    //     this anchor only drives the structural [Code]/[Graph] sections, which
    //     need an exact symbol name. Falls back to the PRIMARY focus item (the
    //     first anchor) when recall found no entity — NOT the joined string,
    //     which would never match a symbol name. When a repository filter is
    //     active the items are already repo-narrowed, so the anchor resolves to
    //     a target-repo symbol.
    let anchor_symbol = recall_items
        .iter()
        .find(|i| i.target_type == RetrievalTargetType::Entity)
        .and_then(|i| by_id.get(&i.target_id).map(|e| e.name.clone()))
        .unwrap_or_else(|| primary_focus.clone());

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
        graph_status.clone()
    } else if links.is_empty() {
        "(none)".to_owned()
    } else {
        links.join("\n")
    };
    // Whether the graph subsystem reported an error (vs OK-but-empty). Captured
    // before `graph_status` is consumed by the Assessment builder below.
    let graph_unavailable = !graph_status.is_empty();

    // Surface the anchor resolution so a caller can see which symbol the
    // structural sections resolved to. For a multi-anchor focus, compare against
    // the PRIMARY anchor (the first item) — the joined string would never equal
    // a single symbol name. Empty when the anchor resolved to the primary.
    let anchor_note = if anchor_symbol == primary_focus {
        String::new()
    } else {
        format!(" (anchor symbol: {anchor_symbol})")
    };

    // Fix 2: surface the multi-anchor list in the header when an array was
    // passed, so a caller can see which anchors drove the packet.
    let anchors_note = if focus_array_passed {
        format!(" (anchors: [{}])", focus_list.join(", "))
    } else {
        String::new()
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

    // Fix 3: Assessment section — escalation handoff. Reports item counts,
    // graph/code neighborhood status, the resolved anchor, missing-evidence
    // notes, and concrete suggested next-step calls referencing the actual
    // focus terms + anchor symbol. Built from the already-computed pieces so
    // there is no extra store/recall cost.
    let assessment = build_assessment(
        &recall_items,
        recall_entity_count,
        recall_chunk_count,
        recall_memory_count,
        &links,
        &code_ctx,
        &anchor_symbol,
        &primary_focus,
        &focus_list,
        graph_unavailable,
    );

    Ok(protocol::text_content(format!(
        "=== Context for '{focus}'{anchor_note}{anchors_note}{shape_note} ===\n\n[Recall]\n{recall_text}\n\n[Graph]\n{graph_text}\n\n[Code]\n{code_ctx:?}{repo_note}\n\n{assessment}"
    )))
}

/// Build the `=== Assessment ===` section (Fix 3: escalation handoff) for a
/// `get_context` packet. Surfaces, in one block at the end of the response:
///
/// - **Items returned:** total + per-target-type breakdown (Entity / Chunk /
///   Memory), over the full recall payload (post-repo-filter), so the caller
///   sees what recall FOUND regardless of display capping.
/// - **Graph neighborhood:** `populated` when the anchor has unified-graph
///   links (doc↔code `describes`/`mentions` edges), `empty` otherwise.
/// - **Anchor symbol:** the resolved symbol name, or `none` with a hint when a
///   NL focus did not resolve to a code symbol.
/// - **Missing evidence:** targeted notes when [Graph] or [Code] is empty, or
///   when recall returned no Entity items. Each note explains the gap + the
///   likely cause. Omitted entirely when nothing is missing (clean packet).
/// - **Suggested next steps:** concrete calls referencing the ACTUAL focus
///   terms + anchor symbol (not generic placeholders): `search`, `symbol_context`
///   (depth+1 for a deeper call chain), `graph_neighbors` (all relationship
///   types, not just the call graph).
fn build_assessment(
    recall_items: &[engram_domain::RetrievalResult],
    entity_count: usize,
    chunk_count: usize,
    memory_count: usize,
    links: &[String],
    code_ctx: &engram_codegraph_queries::SymbolContext,
    anchor_symbol: &str,
    primary_focus: &str,
    focus_list: &[String],
    graph_unavailable: bool,
) -> String {
    let total = recall_items.len();
    let other_count = total.saturating_sub(entity_count + chunk_count + memory_count);

    let graph_populated = !links.is_empty() && !graph_unavailable;
    let code_populated = !code_ctx.callers.is_empty() || !code_ctx.callees.is_empty();

    // Anchor line: name when resolved, else a hint that NL focus did not
    // resolve. The anchor equals the primary focus only when recall found no
    // entity AND no multi-anchor override — i.e. the fallback fired.
    let anchor_line = if code_populated || graph_populated {
        format!("Anchor symbol: {anchor_symbol}")
    } else {
        format!(
            "Anchor symbol: none (focus '{primary_focus}' did not resolve to a code symbol with edges)"
        )
    };

    // Missing-evidence notes — only the gaps that actually fired.
    let mut missing: Vec<String> = Vec::new();
    if links.is_empty() && !graph_unavailable {
        missing.push(
            "No graph relationships found for the anchor. The symbol may not have indexed call edges, or the anchor may not be a code symbol."
                .to_owned(),
        );
    }
    if !code_populated {
        missing.push(
            "No code neighborhood found. Try searching for the exact identifier first, then use it as the focus."
                .to_owned(),
        );
    }
    if entity_count == 0 {
        missing.push(
            "Recall found no code entities. The query may need more specific identifiers, or the code may not be indexed. Try scan_repo first."
                .to_owned(),
        );
    }
    let missing_block = if missing.is_empty() {
        "Missing evidence: none — recall + graph + code all contributed.".to_owned()
    } else {
        let mut s = String::from("Missing evidence:");
        for m in &missing {
            s.push_str("\n  - ");
            s.push_str(m);
        }
        s
    };

    // Suggested next steps reference the ACTUAL focus terms + anchor. Use the
    // primary focus for `search` (the most distinctive single term) and the
    // anchor for the graph expansions.
    let search_term = primary_focus;
    let suggest_symbol_context = if code_populated {
        format!("symbol_context \"{anchor_symbol}\" depth=3 for deeper call chain")
    } else {
        format!("symbol_context \"{anchor_symbol}\" depth=2 to confirm the symbol exists")
    };
    let suggest_graph_neighbors = format!(
        "graph_neighbors \"{anchor_symbol}\" for all relationship types (calls, describes, mentions, belongs_to)"
    );
    let suggest_search = format!("search \"{search_term}\" for exact-identifier lookup");
    let mut steps = format!("Suggested next steps:\n  - {suggest_search}");
    // When a multi-anchor focus was used, suggest batch symbol_context over the
    // anchor list so the caller knows the batch shape is available.
    if focus_list.len() > 1 {
        let list = focus_list
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        steps.push_str(&format!(
            "\n  - symbol_context [{list}] depth=2 (batch: all anchors in one call)"
        ));
    } else {
        steps.push_str(&format!("\n  - {suggest_symbol_context}"));
    }
    steps.push_str(&format!("\n  - {suggest_graph_neighbors}"));

    format!(
        "=== Assessment ===\nItems returned: {total} (Entity: {entity_count}, Chunk: {chunk_count}, Memory: {memory_count}, Other: {other_count})\nGraph neighborhood: {graph_label}\n{anchor_line}\n{missing_block}\n{steps}",
        graph_label = if graph_populated {
            "populated"
        } else {
            "empty"
        },
    )
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
        assert!(diag.contains("3 hits after filter+dedup"), "{diag}");
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
    /// fields default to empty / `false`. `source_score` defaults to `None`;
    /// use [`hit_with_raw`] when a test needs a raw per-lane score.
    fn hit(name: &str, path: Option<&str>, score: f32) -> SearchHit {
        SearchHit {
            entity_id: format!("id-{name}"),
            name: name.to_owned(),
            kind_label: "Function".to_owned(),
            repo: "phanijapps/zbot".to_owned(),
            path: path.map(|p| p.to_owned()),
            retriever: "lexical".to_owned(),
            source_score: None,
            score,
            is_exact: false,
            chunk_excerpt: None,
        }
    }

    /// Like [`hit`] but sets the raw per-lane score (Option B) for render tests
    /// that assert on the `[lane:raw, rrf=…]` / `[lane, raw=…, rrf=…]` format.
    fn hit_with_raw(name: &str, path: Option<&str>, score: f32, raw: f32) -> SearchHit {
        SearchHit {
            source_score: Some(raw),
            ..hit(name, path, score)
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

        // Name contains the query (short query → longer identifier).
        assert!(name_matches_query("loginAnthropic", "login"));

        // Too-short query or name never matches.
        assert!(!name_matches_query("loginAnthropic", "fn"));
        assert!(!name_matches_query("loginAnthropic", ""));

        // Query contains the name — ONLY for ≤2-token queries.
        // 2-token: "Anthropic OAuth" contains "oauth" → match.
        assert!(name_matches_query("oauth", "Anthropic OAuth"));
        // 3+ token NL query: "Anthropic OAuth handler" → NO match (the flood fix).
        assert!(!name_matches_query("oauth", "Anthropic OAuth handler"));

        // Multi-token NL query must NOT match common words (the bug this fixes).
        // "pi coding agent Anthropic login OAuth request model usage" has 9 tokens.
        let nl = "pi coding agent Anthropic login OAuth request model usage";
        assert!(!name_matches_query("request", nl));
        assert!(!name_matches_query("model", nl));
        assert!(!name_matches_query("log", nl));
        assert!(!name_matches_query("m", nl));
        assert!(!name_matches_query("R", nl));

        // But an identifier typed as the query still matches.
        assert!(name_matches_query("anthropicOAuth", "anthropicOAuth"));
        assert!(name_matches_query("loginAnthropic", "loginAnthropic"));
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
    fn inject_exact_matches_marks_existing_without_boosting_score() {
        // Option B: existing matches are marked is_exact but their RRF score is
        // LEFT UNCHANGED (no 1.0 floor). The raw lane score surfaced in rendering
        // shows whether it's a strong match.
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
            "e3".into(),
            make_entity(
                "e3",
                "gamma",
                engram_domain::EntityKind::Struct,
                Some("d.rs"),
            ),
        );

        // alpha is already in hits with a low RRF score (0.05).
        let mut hits = vec![SearchHit {
            entity_id: "e1".into(),
            name: "alpha".into(),
            kind_label: "Function".into(),
            repo: "phanijapps/zbot".into(),
            path: Some("a.rs".into()),
            retriever: "lexical".into(),
            source_score: Some(0.41),
            score: 0.05,
            is_exact: false,
            chunk_excerpt: None,
        }];

        let injected = inject_exact_matches(&mut hits, &by_id, "alpha");
        assert_eq!(injected, 0, "alpha already present → not re-injected");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].is_exact, "alpha marked exact");
        // Score UNCHANGED — no 1.0 boost.
        assert!(
            (hits[0].score - 0.05).abs() < 1e-6,
            "alpha score unchanged (was 0.05, no 1.0 floor)"
        );
        assert_eq!(hits[0].source_score, Some(0.41), "raw lane score preserved");
    }

    #[test]
    fn inject_exact_matches_injects_exact_identifier_at_max_rrf() {
        // Option B: an injected EXACT identifier match (query == name) gets the
        // max RRF score among existing hits — NOT 1.0. It ranks at the top with
        // a realistic score.
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
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

        // One existing hit with score 0.05 → that is the anchor (max RRF).
        let mut hits = vec![hit("unrelated", Some("z.rs"), 0.05)];

        let injected = inject_exact_matches(&mut hits, &by_id, "anthropicOAuth");
        assert_eq!(injected, 1, "anthropicOAuth injected");
        let inj = hits.iter().find(|h| h.name == "anthropicOAuth").unwrap();
        assert!(inj.is_exact);
        // Score = max RRF of existing hits = 0.05, NOT 1.0.
        assert!(
            (inj.score - 0.05).abs() < 1e-6,
            "injected exact match scored at max RRF (0.05), not 1.0"
        );
        assert_eq!(inj.retriever, "exact-match");
        assert!(
            inj.source_score.is_none(),
            "injected hits carry no raw lane score"
        );
        assert_eq!(inj.path.as_deref(), Some("c.rs"));
        // gamma never matched → never injected.
        assert!(hits.iter().all(|h| h.name != "gamma"));
    }

    #[test]
    fn inject_exact_matches_injects_partial_match_at_80_pct_of_anchor() {
        // Option B: a substring/partial match (not exact identifier) gets 80%
        // of the anchor (max RRF). query "login" → name "loginAnthropic".
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        by_id.insert(
            "e9".into(),
            make_entity(
                "e9",
                "loginAnthropic",
                engram_domain::EntityKind::Function,
                Some("l.rs"),
            ),
        );

        // Existing hit at 0.10 → anchor = 0.10.
        let mut hits = vec![hit("unrelated", Some("z.rs"), 0.10)];
        let injected = inject_exact_matches(&mut hits, &by_id, "login");
        assert_eq!(injected, 1);
        let inj = hits.iter().find(|h| h.name == "loginAnthropic").unwrap();
        // Partial match → 0.8 * 0.10 = 0.08.
        assert!(
            (inj.score - 0.08).abs() < 1e-6,
            "partial match at 80% of anchor: expected 0.08, got {}",
            inj.score
        );
    }

    #[test]
    fn inject_exact_matches_anchor_fallback_when_no_existing_hits() {
        // When recall returned nothing (no existing hits to anchor against), the
        // injected match gets DEFAULT_MIN_SCORE * 5 (0.05) so it still clears the
        // 0.01 score floor instead of being dropped.
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        by_id.insert(
            "e2".into(),
            make_entity(
                "e2",
                "lonelyFunc",
                engram_domain::EntityKind::Function,
                Some("l.rs"),
            ),
        );
        let mut hits: Vec<SearchHit> = Vec::new();
        let injected = inject_exact_matches(&mut hits, &by_id, "lonelyFunc");
        assert_eq!(injected, 1);
        assert!(
            (hits[0].score - 0.05).abs() < 1e-6,
            "fallback anchor = 0.05 when no existing hits"
        );
    }

    #[test]
    fn name_exact_match_distinguishes_identifier_from_substring() {
        // Exact: query IS the identifier.
        assert!(name_exact_match("anthropicOAuth", "anthropicOAuth"));
        assert!(name_exact_match("anthropicOAuth", "AnthropicOAuth"));
        // Substring is NOT exact.
        assert!(!name_exact_match("loginAnthropic", "login"));
        // Empty / whitespace.
        assert!(!name_exact_match("alpha", ""));
        assert!(!name_exact_match("alpha", "   "));
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
    fn render_hit_compact_shows_raw_and_rrf_when_source_score_set() {
        // Option B + Fix 4: compact format surfaces the raw lane score, the rrf,
        // AND the within-query normalized score. Format:
        // `name — repo, path [lane:raw, rrf=Y.YY (norm:Z.ZZ)]`.
        let h = hit_with_raw(
            "loginAnthropic",
            Some("packages/ai/src/auth/oauth/anthropic.ts"),
            0.0382,
            0.917,
        );
        // Single-hit result set → divisor == score → norm 1.00.
        let compact = render_hit(&h, false, h.score);
        assert!(
            compact.starts_with("loginAnthropic — zbot, packages/ai/src/auth/oauth/anthropic.ts"),
            "compact format: {compact}"
        );
        // Raw lane score (lexical:0.92) + rrf (0.04) + norm (1.00).
        assert!(
            compact.contains("[lexical:0.92, rrf=0.04 (norm:1.00)]"),
            "raw + rrf + norm bracket: {compact}"
        );
        // Kind + retriever label dropped in compact (lane shows in the bracket).
        assert!(!compact.contains("Function"), "kind dropped: {compact}");
    }

    #[test]
    fn render_hit_compact_shows_only_rrf_when_no_source_score() {
        // When source_score is None, compact shows `[rrf=Y.YY (norm:Z.ZZ)]` only.
        let h = hit("someEntity", Some("src/lib.rs"), 0.031);
        let compact = render_hit(&h, false, h.score);
        assert!(
            compact.contains("[rrf=0.03 (norm:1.00)]"),
            "rrf-only + norm bracket when raw absent: {compact}"
        );
        assert!(
            !compact.contains("lexical"),
            "lane name not duplicated outside bracket: {compact}"
        );
    }

    #[test]
    fn render_hit_diagnostics_shows_lane_raw_rrf() {
        // Diagnostics format: `name (kind) — repo, path [lane, raw=X.XX, rrf=Y.YY (norm:Z.ZZ)]`.
        let h = hit_with_raw(
            "loginAnthropic",
            Some("packages/ai/src/auth/oauth/anthropic.ts"),
            0.0382,
            0.917,
        );
        let diag = render_hit(&h, true, h.score);
        assert!(
            diag.starts_with(
                "loginAnthropic (Function) — phanijapps/zbot, packages/ai/src/auth/oauth/anthropic.ts"
            ),
            "diagnostics format: {diag}"
        );
        assert!(
            diag.contains("[lexical, raw=0.92, rrf=0.04 (norm:1.00)]"),
            "diagnostics bracket with raw + rrf + norm: {diag}"
        );
    }

    #[test]
    fn render_hit_diagnostics_shows_lane_rrf_when_no_source_score() {
        // Diagnostics without raw: `[lane, rrf=Y.YY (norm:Z.ZZ)]` (lane still named).
        let h = hit("someEntity", Some("src/lib.rs"), 0.031);
        let diag = render_hit(&h, true, h.score);
        assert!(
            diag.contains("[lexical, rrf=0.03 (norm:1.00)]"),
            "diagnostics bracket without raw: {diag}"
        );
    }

    #[test]
    fn render_hit_exact_match_injected_shows_tag() {
        // Injected exact-match hit: `[exact-match, rrf=Y.YY (norm:Z.ZZ)]` (no raw lane score).
        let h = SearchHit {
            entity_id: "e1".into(),
            name: "alpha".into(),
            kind_label: "Function".into(),
            repo: "phanijapps/zbot".into(),
            path: Some("a.rs".into()),
            retriever: "exact-match".into(),
            source_score: None,
            score: 0.05,
            is_exact: true,
            chunk_excerpt: None,
        };
        let compact = render_hit(&h, false, h.score);
        assert!(
            compact.contains("[exact-match, rrf=0.05 (norm:1.00)]"),
            "injected exact-match tag in compact: {compact}"
        );
        let diag = render_hit(&h, true, h.score);
        assert!(
            diag.contains("[exact-match, rrf=0.05 (norm:1.00)]"),
            "injected exact-match tag in diagnostics: {diag}"
        );
    }

    #[test]
    fn render_hit_omits_path_gracefully_when_absent() {
        let h = hit("noPath", None, 0.5);
        let compact = render_hit(&h, false, h.score);
        // No ", path" segment — just "name — repo [rrf=… (norm:…)]" (no source_score).
        assert_eq!(compact, "noPath — zbot [rrf=0.50 (norm:1.00)]");
    }

    #[test]
    fn discovery_header_formats_name_kind_repo_path_raw_rrf() {
        // Option B: discovery header surfaces raw lane score + rrf.
        // Format: `name (kind) — repo, path [lane, raw=X.XX, rrf=Y.YY]`.
        // (`recall_item` sets provenance.source = "test" and no fusion_trace, so
        // the lane is "?" and raw is absent → the bracket is `[?, rrf=1.00]`.
        // This asserts the FORMAT path with no raw score; the raw-bearing path
        // is covered by discovery_header_includes_raw_when_trace_sets_it.)
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
        assert!(
            header.contains("[?, rrf=1.00]"),
            "rrf bracket (no raw): {header}"
        );
        // Discovery mode must NOT include the content body.
        assert!(
            !header.contains("content-e1"),
            "no content in discovery: {header}"
        );
    }

    #[test]
    fn discovery_header_includes_raw_when_trace_sets_it() {
        // When the recall item carries a fusion_trace with a raw source_score,
        // the discovery header shows `[lane, raw=X.XX, rrf=Y.YY]`.
        let mut item = recall_item(RetrievalTargetType::Entity, "e1");
        item.fusion_trace = Some(make_trace("vector", Some(0.88)));
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
            header.contains("[vector, raw=0.88, rrf=1.00]"),
            "raw + rrf bracket when trace sets source_score: {header}"
        );
    }

    /// Build a minimal `FusionTrace` for render tests (only `source` + raw
    /// `source_score` vary; the rest is fixture filler — render reads only
    /// those two fields).
    fn make_trace(source: &str, source_score: Option<f32>) -> engram_domain::FusionTrace {
        engram_domain::FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: source.to_owned(),
            source_rank: None,
            source_score,
            score: None,
            rank: None,
            fusion_strategy: None,
            fusion_score: None,
            rerank_strategy: None,
            rerank_score: None,
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }
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

    #[test]
    fn recall_provenance_suffix_shows_raw_and_rrf() {
        // Option B: evidence-mode suffix surfaces raw lane score + rrf.
        let mut item = recall_item(RetrievalTargetType::Entity, "e1");
        item.fusion_trace = Some(make_trace("vector", Some(0.92)));
        item.score.total = 0.024;
        let suffix = recall_provenance_suffix(&item);
        assert!(
            suffix.contains("[vector, raw=0.92, rrf=0.02]"),
            "raw + rrf provenance suffix: {suffix}"
        );
    }

    #[test]
    fn recall_provenance_suffix_shows_only_rrf_when_raw_absent() {
        let mut item = recall_item(RetrievalTargetType::Entity, "e1");
        item.fusion_trace = Some(make_trace("lexical", None));
        item.score.total = 0.031;
        let suffix = recall_provenance_suffix(&item);
        assert!(
            suffix == "[lexical, rrf=0.03]",
            "rrf-only suffix when raw absent: {suffix}"
        );
    }

    #[test]
    fn recall_provenance_suffix_handles_missing_trace() {
        // No fusion_trace at all → `[?, rrf=Y.YY]`.
        let item = recall_item(RetrievalTargetType::Entity, "e1");
        let suffix = recall_provenance_suffix(&item);
        assert_eq!(suffix, "[?, rrf=1.00]", "missing trace fallback: {suffix}");
    }

    // --- chunk-in-search behavior (Fix: chunks + entities) -------------------

    /// Build a `SearchHit` for a Chunk with the given content + optional path +
    /// score. Mirrors [`hit`] for entity hits; sets `chunk_excerpt` to the
    /// bounded excerpt of `content`.
    fn hit_chunk(content: &str, path: Option<&str>, score: f32) -> SearchHit {
        SearchHit {
            entity_id: "chunk-1".to_owned(),
            name: chunk_label(content),
            kind_label: "Chunk".to_owned(),
            repo: "phanijapps/zbot".to_owned(),
            path: path.map(|p| p.to_owned()),
            retriever: "vector".to_owned(),
            source_score: Some(0.88),
            score,
            is_exact: false,
            chunk_excerpt: Some(excerpt(content, CHUNK_EXCERPT_CHARS)),
        }
    }

    #[test]
    fn chunk_label_extracts_first_non_empty_line_truncated() {
        // First non-empty line is the label.
        assert_eq!(
            chunk_label("const TOKEN = 'sk-ant-oat';\nfn main() {}"),
            "const TOKEN = 'sk-ant-oat';"
        );

        // Leading blank lines are skipped.
        assert_eq!(
            chunk_label("\n\n\nexport async function loginAnthropic() {"),
            "export async function loginAnthropic() {"
        );

        // Long first line is truncated to 60 chars.
        let long_line = "x".repeat(200);
        assert_eq!(chunk_label(&long_line).len(), 60);

        // Whitespace-only content → fallback label.
        assert_eq!(chunk_label("   \n  \n"), "(empty chunk)");
        assert_eq!(chunk_label(""), "(empty chunk)");
    }

    #[test]
    fn from_recall_chunk_builds_hit_with_excerpt_and_label() {
        let mut item = recall_item(RetrievalTargetType::Chunk, "chunk-42");
        item.content =
            "const API_KEY = 'sk-ant-oat-12345';\nfetch(url, { headers: { Authorization: Bearer } });"
                .to_owned();
        item.score.total = 0.04;
        item.fusion_trace = Some(make_trace("vector", Some(0.91)));

        let h = SearchHit::from_recall_chunk(&item, Some("src/auth/anthropic.ts"));
        assert_eq!(h.entity_id, "chunk-42");
        assert_eq!(
            h.name, "const API_KEY = 'sk-ant-oat-12345';",
            "name is the first line (chunk_label)"
        );
        assert_eq!(h.kind_label, "Chunk");
        assert_eq!(h.path.as_deref(), Some("src/auth/anthropic.ts"));
        assert_eq!(h.retriever, "vector");
        assert!((h.source_score.unwrap() - 0.91).abs() < 1e-6);
        assert!((h.score - 0.04).abs() < 1e-6);
        assert!(!h.is_exact, "chunks are never is_exact");
        // The excerpt carries the decisive code text.
        let excerpt_text = h.chunk_excerpt.as_ref().expect("chunk has excerpt");
        assert!(
            excerpt_text.contains("sk-ant-oat-12345"),
            "excerpt must surface the credential value: {excerpt_text}"
        );
        assert!(
            excerpt_text.contains("Authorization: Bearer"),
            "excerpt must surface the header: {excerpt_text}"
        );
    }

    #[test]
    fn from_recall_chunk_truncates_excerpt_to_500_chars() {
        let mut item = recall_item(RetrievalTargetType::Chunk, "chunk-long");
        // 1000 chars of content → excerpt is 500 + truncation marker.
        item.content = "A".repeat(1000);
        let h = SearchHit::from_recall_chunk(&item, None);
        let excerpt_text = h.chunk_excerpt.as_ref().unwrap();
        // excerpt() appends "\n... [truncated]" when cutting.
        assert!(
            excerpt_text.contains("... [truncated]"),
            "long chunk excerpt is truncated: {excerpt_text}"
        );
        // The excerpt body (excluding the marker) is at most 500 chars.
        let body = excerpt_text.split("\n... [truncated]").next().unwrap();
        assert!(
            body.chars().count() <= 500,
            "excerpt body ≤ 500 chars: got {}",
            body.chars().count()
        );
    }

    #[test]
    fn render_hit_chunk_appends_excerpt_after_header_compact() {
        let h = hit_chunk(
            "const TOKEN = 'sk-ant-oat-xxxxx';\nconst HOST = 'api.anthropic.com';",
            Some("src/auth.ts"),
            0.04,
        );
        let compact = render_hit(&h, false, h.score);
        // Header line: label — repo, path [vector:raw, rrf=Y.YY (norm:Z.ZZ)]
        let header_line = compact.lines().next().unwrap();
        assert!(
            header_line.contains("const TOKEN = 'sk-ant-oat-xxxxx';"),
            "header shows chunk label: {header_line}"
        );
        assert!(
            header_line.contains("— zbot, src/auth.ts"),
            "header shows repo + path: {header_line}"
        );
        assert!(
            header_line.contains("[vector:0.88, rrf=0.04 (norm:1.00)]"),
            "header shows raw + rrf + norm: {header_line}"
        );
        // Excerpt (after the header) surfaces the decisive code VALUES.
        // The excerpt may span multiple lines, so check the full output.
        assert!(
            compact.contains("sk-ant-oat-xxxxx"),
            "excerpt surfaces the credential: {compact}"
        );
        assert!(
            compact.contains("api.anthropic.com"),
            "excerpt surfaces the host: {compact}"
        );
        // Two distinct blocks: header (line 0) + excerpt (line 1+).
        assert!(
            compact.lines().count() >= 2,
            "compact output has header + excerpt: {compact}"
        );
    }

    #[test]
    fn render_hit_chunk_appends_excerpt_after_header_diagnostics() {
        let h = hit_chunk(
            "async function loginAnthropic() {",
            Some("packages/ai/src/auth/oauth/anthropic.ts"),
            0.04,
        );
        let diag = render_hit(&h, true, h.score);
        // Header includes (Chunk) kind label.
        let header_line = diag.lines().next().unwrap();
        assert!(
            header_line.contains("(Chunk)"),
            "diagnostics header shows Chunk kind: {header_line}"
        );
        assert!(
            header_line.contains("[vector, raw=0.88, rrf=0.04 (norm:1.00)]"),
            "diagnostics bracket: {header_line}"
        );
    }

    #[test]
    fn render_hit_chunk_without_path_omits_path_segment() {
        let h = hit_chunk("const BEARER = 'Bearer xyz';", None, 0.03);
        let compact = render_hit(&h, false, h.score);
        let header_line = compact.lines().next().unwrap();
        // No path between repo and bracket: "label — repo [bracket]"
        // (not "label — repo, path [bracket]"). The bracket follows the repo
        // directly.
        assert!(
            header_line.contains("— zbot ["),
            "bracket immediately follows repo when path is None: {header_line}"
        );
        assert!(
            !header_line.contains(", src"),
            "no source path segment when path is None: {header_line}"
        );
        // Excerpt still present.
        assert!(
            compact.contains("Bearer xyz"),
            "excerpt present even without path: {compact}"
        );
    }

    #[test]
    fn should_keep_target_type_filters_correctly() {
        // Entity always passes.
        assert!(should_keep_target_type(&RetrievalTargetType::Entity, true));
        assert!(should_keep_target_type(&RetrievalTargetType::Entity, false));

        // Chunk passes ONLY when include_chunks is true.
        assert!(
            should_keep_target_type(&RetrievalTargetType::Chunk, true),
            "Chunk kept when include_chunks=true"
        );
        assert!(
            !should_keep_target_type(&RetrievalTargetType::Chunk, false),
            "Chunk filtered when include_chunks=false"
        );

        // Memory, Belief, etc. are ALWAYS filtered (search is code search).
        assert!(!should_keep_target_type(&RetrievalTargetType::Memory, true));
        assert!(!should_keep_target_type(&RetrievalTargetType::Belief, true));
    }

    #[test]
    fn inject_exact_matches_skips_chunk_hits() {
        // A chunk whose label happens to match the query must NOT be marked
        // is_exact — that tag means "identifier match," not "content match."
        let mut by_id: HashMap<String, KnowledgeEntity> = HashMap::new();
        let mut hits = vec![hit_chunk("alphaFunction body text", Some("a.rs"), 0.05)];
        let injected = inject_exact_matches(&mut hits, &by_id, "alphaFunction");
        // No entities in by_id → nothing injected.
        assert_eq!(injected, 0);
        // The chunk is NOT marked is_exact (it's a content label, not a symbol).
        assert!(
            !hits[0].is_exact,
            "chunk hits must not be marked is_exact even if label matches"
        );
    }

    // --- Fix 1: parse_str_or_array (batch-call arg parser) -------------------

    #[test]
    fn parse_str_or_array_accepts_single_string() {
        let args = json!({ "symbol": "loginAnthropic" });
        let out = parse_str_or_array(&args, "symbol").unwrap();
        assert_eq!(out, vec!["loginAnthropic".to_owned()]);
    }

    #[test]
    fn parse_str_or_array_accepts_string_array() {
        let args = json!({ "symbol": ["loginAnthropic", "resolveStoredOAuth", "createClient"] });
        let out = parse_str_or_array(&args, "symbol").unwrap();
        assert_eq!(
            out,
            vec![
                "loginAnthropic".to_owned(),
                "resolveStoredOAuth".to_owned(),
                "createClient".to_owned(),
            ]
        );
    }

    #[test]
    fn parse_str_or_array_rejects_missing_key() {
        let args = json!({});
        let err = parse_str_or_array(&args, "symbol").unwrap_err();
        assert!(
            err.message.contains("symbol is required"),
            "{}",
            err.message
        );
    }

    #[test]
    fn parse_str_or_array_rejects_empty_string_and_empty_array() {
        let err = parse_str_or_array(&json!({ "symbol": "" }), "symbol").unwrap_err();
        assert!(err.message.contains("symbol is required"));

        let err = parse_str_or_array(&json!({ "symbol": [] }), "symbol").unwrap_err();
        assert!(
            err.message.contains("symbol is required"),
            "{}",
            err.message
        );
    }

    #[test]
    fn parse_str_or_array_rejects_array_with_empty_or_non_string_element() {
        // Empty-string element.
        let err = parse_str_or_array(&json!({ "symbol": ["a", ""] }), "symbol").unwrap_err();
        assert!(
            err.message.contains("must contain only non-empty strings"),
            "{}",
            err.message
        );
        // Non-string element (number).
        let err = parse_str_or_array(&json!({ "symbol": ["a", 42] }), "symbol").unwrap_err();
        assert!(
            err.message.contains("must contain only non-empty strings"),
            "{}",
            err.message
        );
    }

    #[test]
    fn parse_str_or_array_rejects_non_string_non_array_value() {
        let err = parse_str_or_array(&json!({ "symbol": 123 }), "symbol").unwrap_err();
        assert!(
            err.message.contains("symbol is required"),
            "{}",
            err.message
        );
        let err = parse_str_or_array(&json!({ "symbol": null }), "symbol").unwrap_err();
        assert!(
            err.message.contains("symbol is required"),
            "{}",
            err.message
        );
    }

    // --- Fix 4: score normalization within a result set ----------------------

    /// Fix 4: the within-query normalized score is `raw_rrf / max_rrf`, so the
    /// top result renders as norm:1.00 and the rest scale proportionally. This
    /// gives the agent meaningful separation (1.00 vs 0.80) without changing
    /// the underlying fusion. Verified via the bracket format in `score_bracket`.
    #[test]
    fn score_bracket_normalizes_within_result_set() {
        // Two hits: top at 0.05, second at 0.04. Max = 0.05.
        // norm(top)    = 0.05 / 0.05 = 1.00
        // norm(second) = 0.04 / 0.05 = 0.80
        let top = hit_with_raw("alpha", Some("a.rs"), 0.05, 0.92);
        let second = hit_with_raw("beta", Some("b.rs"), 0.04, 0.88);
        let max = 0.05_f32;

        let bracket_top = score_bracket(&top, false, max);
        let bracket_second = score_bracket(&second, false, max);
        assert!(
            bracket_top.contains("rrf=0.05 (norm:1.00)"),
            "top result normalizes to 1.00: {bracket_top}"
        );
        assert!(
            bracket_second.contains("rrf=0.04 (norm:0.80)"),
            "second result scales proportionally to 0.80: {bracket_second}"
        );
    }

    /// Fix 4: a non-positive divisor (empty result set edge) yields norm:0.00
    /// rather than NaN — `score_bracket` guards the division.
    #[test]
    fn score_bracket_guard_against_zero_divisor() {
        let h = hit_with_raw("alpha", Some("a.rs"), 0.0, 0.92);
        let bracket = score_bracket(&h, false, 0.0);
        assert!(
            bracket.contains("(norm:0.00)"),
            "zero divisor → norm 0.00 (no NaN): {bracket}"
        );
    }

    // --- Fix 3: build_assessment (escalation handoff) -------------------------

    /// Build a `SymbolContext` for assessment tests. `with_edges` controls
    /// whether the code neighborhood is populated (callers/callees non-empty).
    fn make_symbol_context(with_edges: bool) -> engram_codegraph_queries::SymbolContext {
        engram_codegraph_queries::SymbolContext {
            callers: if with_edges {
                vec!["callerA".to_owned()]
            } else {
                Vec::new()
            },
            callees: if with_edges {
                vec!["calleeB".to_owned()]
            } else {
                Vec::new()
            },
            community: Some(0),
        }
    }

    #[test]
    fn build_assessment_reports_counts_and_populated_status() {
        // Two entity recall items, one chunk, populated graph + code.
        let items = vec![
            recall_item(RetrievalTargetType::Entity, "e1"),
            recall_item(RetrievalTargetType::Entity, "e2"),
            recall_item(RetrievalTargetType::Chunk, "c1"),
        ];
        let links = vec!["loginAnthropic -[describes]-> OAuth".to_owned()];
        let code_ctx = make_symbol_context(true);
        let focus_list = vec!["loginAnthropic".to_owned()];

        let assessment = build_assessment(
            &items,
            2, // entity
            1, // chunk
            0, // memory
            &links,
            &code_ctx,
            "loginAnthropic",
            "loginAnthropic",
            &focus_list,
            false, // graph available
        );

        assert!(
            assessment.contains("Items returned: 3 (Entity: 2, Chunk: 1, Memory: 0"),
            "counts line: {assessment}"
        );
        assert!(
            assessment.contains("Graph neighborhood: populated"),
            "populated graph: {assessment}"
        );
        assert!(
            assessment.contains("Anchor symbol: loginAnthropic"),
            "anchor line: {assessment}"
        );
        // Nothing missing → the "none" message.
        assert!(
            assessment.contains("Missing evidence: none"),
            "no missing evidence: {assessment}"
        );
        // Suggested next steps reference the actual focus + anchor.
        assert!(
            assessment.contains("search \"loginAnthropic\""),
            "search suggestion references focus: {assessment}"
        );
        assert!(
            assessment.contains("symbol_context \"loginAnthropic\" depth=3"),
            "symbol_context suggestion references anchor: {assessment}"
        );
        assert!(
            assessment.contains("graph_neighbors \"loginAnthropic\""),
            "graph_neighbors suggestion references anchor: {assessment}"
        );
    }

    #[test]
    fn build_assessment_notes_missing_evidence_when_graph_and_code_empty() {
        // No recall items, no links, no code edges → all three missing-evidence
        // notes fire.
        let items: Vec<RetrievalResult> = Vec::new();
        let links: Vec<String> = Vec::new();
        let code_ctx = make_symbol_context(false);
        let focus_list = vec!["someConcept".to_owned()];

        let assessment = build_assessment(
            &items,
            0,
            0,
            0,
            &links,
            &code_ctx,
            "someConcept",
            "someConcept",
            &focus_list,
            false,
        );

        assert!(
            assessment.contains("Graph neighborhood: empty"),
            "empty graph: {assessment}"
        );
        assert!(
            assessment.contains("Anchor symbol: none"),
            "anchor did not resolve: {assessment}"
        );
        // All three missing-evidence notes fire.
        assert!(
            assessment.contains("No graph relationships found"),
            "graph-missing note: {assessment}"
        );
        assert!(
            assessment.contains("No code neighborhood found"),
            "code-missing note: {assessment}"
        );
        assert!(
            assessment.contains("Recall found no code entities"),
            "recall-empty note: {assessment}"
        );
    }

    #[test]
    fn build_assessment_multi_anchor_suggests_batch_symbol_context() {
        // When the focus list has >1 anchor, the suggested next step promotes
        // the batch symbol_context shape instead of the single-symbol depth=3
        // suggestion.
        let items = vec![recall_item(RetrievalTargetType::Entity, "e1")];
        let links = vec!["a -[calls]-> b".to_owned()];
        let code_ctx = make_symbol_context(true);
        let focus_list = vec!["loginAnthropic".to_owned(), "resolveStoredOAuth".to_owned()];

        let assessment = build_assessment(
            &items,
            1,
            0,
            0,
            &links,
            &code_ctx,
            "loginAnthropic",
            "loginAnthropic",
            &focus_list,
            false,
        );

        assert!(
            assessment.contains("symbol_context [\"loginAnthropic\", \"resolveStoredOAuth\"]"),
            "multi-anchor suggestion promotes batch symbol_context: {assessment}"
        );
        assert!(
            assessment.contains("(batch: all anchors in one call)"),
            "batch hint present: {assessment}"
        );
    }
}
