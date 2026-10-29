//! `fetch_content`: return one indexed file's content — verbatim from disk
//! when possible, stitched from stored chunks otherwise.
//!
//! Modes (`mode` arg, default `auto`):
//! - **auto**: verbatim source read when the scan root is resolvable
//!   ([`crate::scan_roots`]) and the file exists on disk; falls back to
//!   chunk stitching with an explicit note otherwise.
//! - **verbatim**: require the on-disk read; error with the reason when the
//!   root/file cannot be resolved (no silent fallback).
//! - **chunks**: the legacy best-effort reconstruction (doc sections joined;
//!   code containers store doc-comment + signature only).
//!
//! `symbol` (optional): slice the verbatim source to one symbol's stored
//! line span (the chunk carrying that entity) — symbol-scoped fetch without
//! whole-file payloads. Requires verbatim resolution; ignored in `chunks`
//! mode.
//!
//! Content is capped at [`DEFAULT_MAX_BYTES`] (100 KiB) by default; pass
//! `max_bytes` to raise/lower it within [`MAX_BYTES_CEILING`]. Reads every
//! chunk in the scope via [`KnowledgeQuery::list_chunks`] and filters by
//! `location.path`. Multi-repo scopes disambiguate via the `repository`
//! filter.

use engram_domain::KnowledgeChunkKind;
use futures::executor::block_on;
use serde_json::{Value, json};

use crate::app::App;
use crate::codegraph::source_matches_repository;
use crate::protocol;
use crate::registry::ToolError;
use crate::tools::req_str;

/// Default content cap: 100 KiB. Configurable per call via `max_bytes`.
pub const DEFAULT_MAX_BYTES: usize = 100 * 1024;
/// Hard ceiling for `max_bytes` overrides (matches the scanner's 1 MiB limit).
pub const MAX_BYTES_CEILING: usize = 1024 * 1024;

/// Canonical participating chunks (corrective r4, canonical 10 / original
/// 11): a chunk participates in the EXACT path only when its ID carries
/// DURABLE LINEAGE whose (repository, generation) matches a PUBLISHED
/// eligible binding in this scope — never a metadata stamp, and never an
/// unstamped row. Participants span at most one published generation per
/// repository (retired generations' lineages do not match any eligible
/// binding), so the origin, the symbol spans and the served revision all
/// come from the SAME canonical current generation within one read.
fn canonical_participating_chunks<'a>(
    app: &App,
    matched: &[&'a engram_domain::KnowledgeChunk],
) -> Result<(Vec<&'a engram_domain::KnowledgeChunk>, Option<(String, String)>), String> {
    let Ok(coordinator) = app.provider.code_bindings() else {
        // No binding coordinator configured: no canonical participant can be
        // established and the exact path does not apply (the verbatim path
        // is the caller's fallback). This is not an ambiguity error.
        return Ok((Vec::new(), None));
    };
    let view = futures::executor::block_on(coordinator.eligibility_view(&app.scope, None))
        .map_err(|error| format!("eligibility resolution failed: {error}"))?;
    let published: std::collections::BTreeSet<(String, String)> = view
        .eligible_bindings()
        .into_iter()
        .map(|binding| (binding.repo_id, binding.generation_id))
        .collect();
    let mut participants: Vec<&'a engram_domain::KnowledgeChunk> = Vec::new();
    let mut origin: Option<(String, String)> = None;
    for chunk in matched {
        let Some((repo, generation)) = view.code_lineage_of(chunk.id.as_str()) else {
            // No durable lineage: unstamped or forged-stamp rows are not
            // admissible participants.
            continue;
        };
        if !published.contains(&(repo.to_owned(), generation.to_owned())) {
            // Durable lineage on a NON-published generation (retained old
            // generation): not a participant.
            continue;
        }
        let candidate = (repo.to_owned(), generation.to_owned());
        if let Some(existing) = &origin
            && existing != &candidate
        {
            return Err("matched chunks span multiple published repository generations".into());
        }
        origin = Some(candidate);
        participants.push(*chunk);
    }
    Ok((participants, origin))
}

/// Resolves `symbol` against the matched (same-generation) chunks and maps
/// its line span to byte offsets in the retained blob (finding 11).
///
/// `Ok(None)` — no symbol requested. `Ok(Some((start, end)))` — the unique
/// byte span. `Err(reason)` — the symbol is missing, matches nothing in this
/// generation, or is ambiguous across chunks.
fn exact_symbol_span(
    chunks: &[&engram_domain::KnowledgeChunk],
    symbol: Option<&str>,
    blob: &[u8],
) -> Result<Option<(usize, usize)>, String> {
    let Some(symbol) = symbol else {
        return Ok(None);
    };
    let mut spans: Vec<(u32, u32)> = chunks
        .iter()
        .filter_map(|chunk| {
            let names = chunk
                .entities
                .iter()
                .filter_map(|entity| entity.name.as_deref())
                .chain(
                    chunk
                        .location
                        .as_ref()
                        .and_then(|location| location.anchor.as_deref())
                        .into_iter(),
                );
            let names: Vec<&str> = names.collect();
            if names.is_empty() || !names.iter().any(|name| *name == symbol) {
                return None;
            }
            let location = chunk.location.as_ref()?;
            Some((
                location.start_line.unwrap_or(1),
                location
                    .end_line
                    .unwrap_or(location.start_line.unwrap_or(1)),
            ))
        })
        .collect();
    spans.sort();
    spans.dedup();
    match spans.as_slice() {
        [] => Err(format!(
            "symbol '{symbol}' has no chunk in the indexed generation for this path"
        )),
        [only] => {
            let start_line = only.0.max(1) as usize;
            let end_line = (only.1.max(only.0.max(1))) as usize;
            // Map 1-based inclusive line numbers to byte offsets over the
            // retained blob (line separators are \n and \r\n).
            let mut line_start_offsets: Vec<usize> = Vec::with_capacity(end_line + 2);
            line_start_offsets.push(0);
            let mut index = 0usize;
            while index < blob.len() {
                if blob[index] == b'\n' {
                    line_start_offsets.push(index + 1);
                }
                index += 1;
            }
            let start = *line_start_offsets
                .get(start_line - 1)
                .ok_or("symbol start line is outside the retained blob")?;
            let end = line_start_offsets
                .get(end_line)
                .copied()
                .unwrap_or(blob.len());
            if start > end || end > blob.len() {
                return Err("symbol span is outside the retained blob bounds".into());
            }
            Ok(Some((start, end)))
        }
        _ => Err(format!(
            "symbol '{symbol}' is ambiguous across {len} chunks in the indexed generation",
            len = spans.len()
        )),
    }
}

/// The `fetch_content` tool handler.
pub fn fetch_content(app: &App, args: &Value) -> Result<Value, ToolError> {
    let path = normalize_path(req_str(args, "path")?);
    let repository = args.get("repository").and_then(|v| v.as_str());
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("auto");
    if !matches!(mode, "auto" | "verbatim" | "chunks") {
        return Err(ToolError::new(
            -32602,
            format!("mode must be one of auto|verbatim|chunks, got {mode:?}"),
        ));
    }
    let symbol = args
        .get("symbol")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let max_bytes = args
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_BYTES)
        .clamp(1, MAX_BYTES_CEILING);

    let query = app
        .provider
        .require_knowledge_query()
        .map_err(|e| ToolError::new(-32603, format!("knowledge store unavailable: {e}")))?;
    let chunks = block_on(query.list_chunks(&app.scope))
        .map_err(|e| ToolError::new(-32603, format!("chunk listing failed: {e}")))?;

    // Match by normalized location path (+ optional repository filter, the
    // same matching `search` uses for provenance).
    let matched: Vec<&engram_domain::KnowledgeChunk> = chunks
        .iter()
        .filter(|c| {
            c.location
                .as_ref()
                .and_then(|l| l.path.as_deref())
                .map(|p| normalize_path(p) == path)
                .unwrap_or(false)
                && repository
                    .map(|r| source_filter_matches(&c.provenance.source, r))
                    .unwrap_or(true)
        })
        .collect();

    if matched.is_empty() {
        return Err(ToolError::new(
            -32602,
            format!(
                "no indexed chunks for path '{path}' in this scope — scan_repo (code) or \
                 index_docs (docs) first, then fetch_content"
            ),
        ));
    }

    // Disambiguate multi-repo scopes: same rel path indexed from several
    // sources. Surface the candidates instead of silently mixing content.
    let sources: Vec<&str> = {
        let mut s: Vec<&str> = matched
            .iter()
            .map(|c| c.provenance.source.as_str())
            .collect();
        s.sort_unstable();
        s.dedup();
        s
    };
    if sources.len() > 1 {
        let candidates: Vec<Value> = sources
            .iter()
            .map(|s| {
                let count = matched.iter().filter(|c| c.provenance.source == *s).count();
                json!({ "repository": s, "chunk_count": count })
            })
            .collect();
        return Ok(protocol::text_content(
            json!({
                "status": "ambiguous",
                "path": path,
                "message": "path indexed from multiple sources — pass the exact `repository` value \
                            of one candidate (or the search-style \"org/name\" for git-backed sources)",
                "candidates": candidates,
            })
            .to_string(),
        ));
    }

    // Source order: by source line span; chunks without a start line sort
    // last but keep listing order (stable sort).
    let mut verbatim_fallback_note: Option<String> = None;
    let mut ordered: Vec<&engram_domain::KnowledgeChunk> = matched;
    ordered.sort_by_key(|c| {
        (
            c.location
                .as_ref()
                .and_then(|l| l.start_line)
                .unwrap_or(u32::MAX),
            c.location
                .as_ref()
                .and_then(|l| l.end_line)
                .unwrap_or(u32::MAX),
        )
    });

    // Kind: any symbol/code chunk ⇒ code; otherwise document chunks ⇒ doc.
    let is_code = ordered.iter().any(|c| {
        matches!(
            c.kind,
            KnowledgeChunkKind::CodeSymbol | KnowledgeChunkKind::CodeBlock
        )
    });
    let kind = if is_code { "code" } else { "doc" };

    // --- Bound code: EXACT retained committed bytes (task_017) -----------
    // The exact path is reserved for CANONICALLY PARTICIPATING chunks: ids
    // with durable lineage on a PUBLISHED generation (corrective r4,
    // canonical 10). Unstamped rows and forged stamps are never admissible —
    // they cannot select the origin, a span, or the served revision, and a
    // missing snapshot stays an error in auto and verbatim modes alike
    // (REQ-006: exact content or an explicit unavailable error, never a
    // checkout masquerading as the revision).
    let (participants, bound_origin) = canonical_participating_chunks(app, &ordered).map_err(|reason| {
        ToolError::new(
            -32602,
            format!("bound source origin is ambiguous: {reason}"),
        )
    })?;
    // A row that asserts bound-code metadata or has durable (but retired)
    // lineage cannot fall through to verbatim/stitching when no current
    // canonical participant exists. That would resurrect old-only or forged
    // rows as ordinary content.
    let has_bound_claim = ordered.iter().any(|chunk| {
        chunk.metadata.as_ref().is_some_and(|metadata| {
            metadata.contains_key("codeRepoId") || metadata.contains_key("codeGenerationId")
        })
    });
    if participants.is_empty() && has_bound_claim {
        return Err(ToolError::new(
            -32602,
            "bound source has no current canonical participants",
        ));
    }
    // Symbol/span selection uses ONLY the participants.
    let span_chunks: Vec<&engram_domain::KnowledgeChunk> = if bound_origin.is_some() {
        participants.clone()
    } else {
        Vec::new()
    };
    if mode != "chunks"
        && let Some((repo_id, generation_id)) = bound_origin
    {
        // Corrective r2 (finding 11): the exact path honors SYMBOL selection
        // from the SAME generation (missing/ambiguous symbols deny), slices
        // the retained bytes at same-OID byte offsets, and reports truthful
        // digests: `content_sha256_full` hashes the whole retained blob and
        // `content_sha256` hashes the bytes ACTUALLY RETURNED after caps.
        // Non-UTF-8 bytes are never lossily decoded: they are returned as an
        // explicitly labeled hex encoding of the exact bytes.
        let actor = engram_domain::Actor {
            id: engram_domain::Id::from("engram-mcp-fetch-content"),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        };
        let full_view = futures::executor::block_on(app.provider.code_fetch_exact_at(
            &actor,
            &app.scope,
            &repo_id,
            &generation_id,
            &path,
            None,
        ));
        let full_view = match full_view {
            Ok(view) => view,
            Err(error) => {
                return Err(ToolError::new(
                    -32603,
                    format!("exact retained source unavailable: {error}"),
                ));
            }
        };
        // Symbol selection: canonical PARTICIPATING chunks naming the symbol
        // decide the byte span; exactly one distinct span is required.
        // Unstamped/forged impostors are not participants and can neither
        // select nor ambiguate a span.
        let span = match exact_symbol_span(&span_chunks, symbol, &full_view.bytes) {
            Ok(span) => span,
            Err(reason) => {
                return Err(ToolError::new(
                    -32602,
                    format!("symbol selection failed: {reason}"),
                ));
            }
        };
        let (served_bytes, span_json) = match span {
            None => (full_view.bytes.clone(), serde_json::Value::Null),
            Some((start, end)) => (
                full_view.bytes[start..end].to_vec(),
                json!({ "start": start, "end": end }),
            ),
        };
        let total_bytes = served_bytes.len();
        let served_digest_before_cap = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&served_bytes);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        // Byte-exact cap: truncate on the returned BYTE budget; UTF-8 safety
        // is handled by the encoding branch below (no lossy replacement).
        let (returned, truncated) = if served_bytes.len() > max_bytes {
            (served_bytes[..max_bytes].to_vec(), true)
        } else {
            (served_bytes.clone(), false)
        };
        let served_digest = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&returned);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let (encoding, content) = match String::from_utf8(returned.clone()) {
            Ok(text) => ("utf-8", text),
            Err(_) => {
                // Explicit non-UTF-8 handling: hex-encode the exact bytes —
                // no lossy replacement that would change them.
                (
                    "hex",
                    returned
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>(),
                )
            }
        };
        return Ok(protocol::text_content(
            json!({
                "path": path,
                "repository": repo_id,
                "kind": kind,
                "source": "exact",
                "revision": full_view.commit_oid,
                "generation": generation_id,
                "span": span_json,
                "content_sha256_full": full_view.full_content_digest,
                "content_sha256": served_digest,
                "content_sha256_served_precap": served_digest_before_cap,
                "chunk_count": ordered.len(),
                "total_bytes": total_bytes,
                "returned_bytes": returned.len(),
                "max_bytes": max_bytes,
                "truncated": truncated,
                "encoding": encoding,
                "content": content,
                "note": "exact retained committed bytes for the indexed revision (no checkout fallback); content_sha256 hashes the returned bytes, content_sha256_full the whole blob",
            })
            .to_string(),
        ));
    }

    // --- Verbatim source read (EN-14/EN-16 fix) -------------------------
    // The stored chunk text is a retrieval view, not canonical source
    // (containers keep doc + signature only). When the scan root resolves,
    // read the file from disk at call time — byte-exact, never stale-cache.
    if mode != "chunks" {
        match verbatim_slice(app, sources[0], &path, symbol, &ordered) {
            VerbatimOutcome::Content {
                content,
                first_line,
                last_line,
            } => {
                let total_bytes = content.len();
                let (content, truncated) = truncate_bytes(&content, max_bytes);
                return Ok(protocol::text_content(
                    json!({
                        "path": path,
                        "repository": sources[0],
                        "kind": kind,
                        "source": "verbatim",
                        "chunk_count": ordered.len(),
                        "lines": { "first": first_line, "last": last_line },
                        "total_bytes": total_bytes,
                        "returned_bytes": content.len(),
                        "max_bytes": max_bytes,
                        "truncated": truncated,
                        "content": content,
                        "note": "verbatim on-disk source, read at call time (scan-root registry)",
                    })
                    .to_string(),
                ));
            }
            VerbatimOutcome::Unresolvable(reason) => {
                if mode == "verbatim" {
                    return Err(ToolError::new(
                        -32602,
                        format!("verbatim read unavailable: {reason}"),
                    ));
                }
                // auto: fall through to chunk stitching with the reason noted.
                verbatim_fallback_note = Some(reason.to_owned());
            }
        }
    }

    // Stitch: skip graph-only anchors (empty text — containers with neither
    // doc comment nor signature). Docs join with a blank line (section
    // boundaries); code joins with a single newline (line spans are
    // contiguous-ish).
    //
    // Corrective r4 (canonical 10, fallback consumption): when the path is
    // CANONICALLY BOUND (durable-lineage participants exist), the fallback
    // reconstruction consumes ONLY those participants — old-generation,
    // unstamped and forged-stamp chunks can never inject fallback content
    // for a bound path. Unbound paths keep the historical stitching.
    let fallback_chunks: Vec<&engram_domain::KnowledgeChunk> = if participants.is_empty() {
        ordered.clone()
    } else {
        participants.clone()
    };
    let separator = if is_code { "\n" } else { "\n\n" };
    let parts: Vec<&str> = fallback_chunks
        .iter()
        .filter(|c| !c.text.is_empty())
        .map(|c| c.text.as_str())
        .collect();
    let full = parts.join(separator);
    let total_bytes = full.len();
    let (content, truncated) = truncate_bytes(&full, max_bytes);

    let first_line = fallback_chunks
        .iter()
        .find_map(|c| c.location.as_ref().and_then(|l| l.start_line));
    let last_line = fallback_chunks
        .iter()
        .rev()
        .find_map(|c| c.location.as_ref().and_then(|l| l.end_line));

    let mut body = json!({
        "path": path,
        "repository": sources[0],
        "kind": kind,
        "chunk_count": fallback_chunks.len(),
        "lines": { "first": first_line, "last": last_line },
        "total_bytes": total_bytes,
        "returned_bytes": content.len(),
        "max_bytes": max_bytes,
        "truncated": truncated,
        "content": content,
    });
    if let Some(reason) = verbatim_fallback_note {
        body["source"] = json!("chunks (fallback)");
        body["note"] = json!(format!(
            "verbatim source unavailable ({reason}) — code stitching is best-effort: \
             container chunks store doc-comment + signature only; treat as a \
             reconstruction, not exact source"
        ));
    } else if is_code {
        body["source"] = json!("chunks");
        body["note"] = json!(
            "code stitching is best-effort: container chunks store doc-comment + \
             signature only — pass mode=verbatim for on-disk source when the \
             scan root is resolvable"
        );
    }
    Ok(protocol::text_content(body.to_string()))
}

/// Outcome of a verbatim read attempt.
enum VerbatimOutcome {
    Content {
        content: String,
        first_line: u32,
        last_line: u32,
    },
    Unresolvable(&'static str),
}

/// Read the file verbatim from the scan root, optionally sliced to one
/// symbol's stored line span. `ordered` supplies the span lookup for
/// `symbol` (the chunk carrying the entity whose name/tail matches).
fn verbatim_slice(
    app: &App,
    source: &str,
    path: &str,
    symbol: Option<&str>,
    ordered: &[&engram_domain::KnowledgeChunk],
) -> VerbatimOutcome {
    use engram_domain::RetrievalTargetType;
    let _ = RetrievalTargetType::Entity; // (keeps imports honest in tests)
    let Some(root) = crate::scan_roots::resolve(&app.storage_dir, source) else {
        return VerbatimOutcome::Unresolvable("scan root not recorded for this source");
    };
    let abs = root.join(path);
    let Ok(raw) = std::fs::read_to_string(&abs) else {
        return VerbatimOutcome::Unresolvable("file missing or unreadable under the recorded root");
    };
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    let total = lines.len() as u32;
    let (content, first_line, last_line) = if let Some(symbol) = symbol {
        match symbol_span(app, symbol, path, ordered) {
            Some((start, end)) => {
                let s = (start.saturating_sub(1)) as usize;
                let e = (end as usize).min(lines.len());
                (
                    lines.get(s..e).map(|l| l.concat()).unwrap_or_default(),
                    start,
                    end,
                )
            }
            None => {
                return VerbatimOutcome::Unresolvable(
                    "symbol not found among this path's indexed chunks",
                );
            }
        }
    } else {
        (raw.clone(), 1, total)
    };
    VerbatimOutcome::Content {
        content,
        first_line,
        last_line,
    }
}

/// Resolve `symbol` to a 1-based inclusive line span: the chunk (on this
/// path) that carries the matching entity ref. Falls back to the chunk whose
/// text starts with a plausible declaration of the name (none today — refs
/// are attached at ingest).
fn symbol_span(
    app: &App,
    symbol: &str,
    _path: &str,
    ordered: &[&engram_domain::KnowledgeChunk],
) -> Option<(u32, u32)> {
    let query = app.provider.require_knowledge_query().ok()?;
    let entities = futures::executor::block_on(query.list_entities(&app.scope)).ok()?;
    let needle = symbol.to_lowercase();
    let entity = entities.iter().find(|e| {
        let name = e.name.to_lowercase();
        name == needle || name.ends_with(&format!("::{needle}"))
    })?;
    let id = entity.id.to_string();
    ordered
        .iter()
        .find(|c| {
            c.entities
                .iter()
                .any(|r| r.id.as_ref().is_some_and(|i| i.to_string() == id))
        })
        .and_then(|c| c.location.as_ref())
        .and_then(|l| l.start_line.zip(l.end_line))
}

/// Trim a leading `./` and trailing slashes so scanner-stamped rel paths and
/// caller-supplied paths agree.
fn normalize_path(p: &str) -> String {
    let trimmed = p.trim_start_matches("./");
    trimmed.trim_end_matches('/').to_owned()
}

/// Repository filter match: exact source equality (bare source names —
/// `"engram-mcp-scan"`, `"engram-mcp"` — carry no `org/name` core, so the
/// search-style matcher alone can never resolve them) or the flexible
/// `search`-style `org/name` substring match for git-backed sources.
fn source_filter_matches(source: &str, repository: &str) -> bool {
    source_matches_repository(source, repository) || source.eq_ignore_ascii_case(repository)
}

/// Cut `text` to at most `max_bytes` on a UTF-8 char boundary.
/// Returns `(cut, was_truncated)`.
fn truncate_bytes(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tests::test_app;
    use serde_json::json;

    fn scan(app: &App, repo: &std::path::Path) {
        crate::codegraph::scan_repo(app, &json!({ "path": repo.to_str().unwrap() })).unwrap();
    }

    fn content_of(result: &Value) -> Value {
        let text = result["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    // ---------- task_017: bound-code exact retained source ---------------

    use crate::ontology::{OntologyConfig, TaxonomyConfig};
    use engram_domain::{CodeGeneration, SourceDocument, SourceDocumentKind};
    use engram_ingest::git_source::{
        CommittedFile, ResolvedCommit, RetainedSnapshotReader, retain_snapshot,
    };
    use engram_integration::EngramProviderBuilder;
    use engram_integration::code_binding::{
        CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
    };
    use engram_knowledge::{CodeBindingRepository, KnowledgeRepository};
    use engram_knowledge::{CoreError, CoreResult};

    struct PermissiveAuthorizer;
    #[async_trait::async_trait]
    impl CodeBindingAuthorizer for PermissiveAuthorizer {
        async fn authorize(
            &self,
            _actor: &engram_domain::Actor,
            _scope: &engram_domain::Scope,
            _repo: &engram_domain::RepoId,
            _action: CodeAuthorityAction,
        ) -> CoreResult<()> {
            Ok(())
        }
    }

    /// Typed in-memory double of the NEUTRAL `CodeBindingRepository` port.
    ///
    /// The durable SQLite cell driving this same chain end to end is proven
    /// in `engram-integration`'s `bound_source_status` target (real store,
    /// real receipts, real publish); this double exists only so the MCP
    /// handler's own dispatch can be tested inside the engine-neutral src
    /// tree (the ADR-0022 gate forbids engine type names here). It holds one
    /// published binding + generation record and nothing else.
    struct InMemoryBindings {
        binding: engram_domain::CodeBinding,
        generation: CodeGeneration,
        /// A retained-but-superseded generation (mixed-generation fixtures).
        retained_old: Option<CodeGeneration>,
        /// Durable staged lineage for ADDITIONAL participating chunks (the
        /// canonical origin selector reads chunk lineage, never stamps).
        extra_lineage: std::sync::Mutex<Vec<engram_knowledge::TargetLineage>>,
        descriptor_digest: std::sync::Mutex<Option<String>>,
        live_lease: std::sync::Mutex<Option<engram_knowledge::CodeReadLease>>,
    }

    impl InMemoryBindings {
        fn published(commit_oid: &str) -> Self {
            Self {
                binding: engram_domain::CodeBinding {
                    scope: crate::scope::project_scope("bound-project", "default"),
                    repo_id: engram_domain::RepoId("bound-repo".to_owned()),
                    branch_ref: "refs/heads/main".to_owned(),
                    version: 1,
                    active_generation: Some("gen-1".to_owned()),
                    epoch: 1,
                },
                generation: CodeGeneration {
                    id: "gen-1".to_owned(),
                    binding_version: 1,
                    commit_oid: commit_oid.to_owned(),
                    snapshot_digest: "b".repeat(64),
                    parser_fingerprint: "parser".to_owned(),
                    filter_fingerprint: "filter".to_owned(),
                },
                descriptor_digest: std::sync::Mutex::new(None),
                live_lease: std::sync::Mutex::new(None),
                retained_old: None,
                extra_lineage: std::sync::Mutex::new(Vec::new()),
            }
        }

        /// Records durable lineage for ADDITIONAL participating chunks.
        fn with_chunk_lineage(&self, entries: &[(String, String)]) {
            let mut lineage = self.extra_lineage.lock().expect("lineage lock");
            for (record_id, generation_id) in entries {
                lineage.push(engram_knowledge::TargetLineage {
                    record_id: record_id.clone(),
                    repo_id: self.binding.repo_id.0.clone(),
                    generation_id: generation_id.clone(),
                });
            }
        }

        /// Two retained generations with gen-2 CURRENT (mixed-generation
        /// resolution fixtures): gen-1's snapshot is retained but no longer
        /// active; gen-2 is the published active generation.
        fn published_two_generations(old_commit_oid: &str, current_commit_oid: &str) -> Self {
            let mut bindings = Self::published(current_commit_oid);
            // The CURRENT (published active) generation is gen-2; gen-1's
            // snapshot stays retained but superseded.
            bindings.generation.id = "gen-2".to_owned();
            bindings.binding.epoch = 2;
            bindings.binding.active_generation = Some("gen-2".to_owned());
            bindings.retained_old = Some(CodeGeneration {
                id: "gen-1".to_owned(),
                binding_version: 1,
                commit_oid: old_commit_oid.to_owned(),
                snapshot_digest: "c".repeat(64),
                parser_fingerprint: "parser".to_owned(),
                filter_fingerprint: "filter".to_owned(),
            });
            bindings
        }

        /// Records the durable descriptor digest the exact chain validates
        /// against (the durable-store equivalent is proven in
        /// bound_source_status).
        fn set_descriptor_digest(&self, digest: String) {
            *self.descriptor_digest.lock().expect("digest lock") = Some(digest);
        }
    }

    #[async_trait::async_trait]
    impl engram_knowledge::CodeReadLeases for InMemoryBindings {
        async fn acquire_read_lease(
            &self,
            lease: engram_knowledge::CodeReadLease,
            _now: engram_domain::Timestamp,
        ) -> CoreResult<engram_knowledge::CodeReadLease> {
            *self.live_lease.lock().expect("lease lock") = Some(lease.clone());
            Ok(lease)
        }
        async fn renew_read_lease(
            &self,
            lease: &engram_knowledge::CodeReadLease,
            _now: engram_domain::Timestamp,
            expires_at: engram_domain::Timestamp,
        ) -> CoreResult<engram_knowledge::CodeReadLease> {
            let mut renewed = lease.clone();
            renewed.expires_at = expires_at;
            *self.live_lease.lock().expect("lease lock") = Some(renewed.clone());
            Ok(renewed)
        }
        async fn release_read_lease(
            &self,
            _lease: &engram_knowledge::CodeReadLease,
        ) -> CoreResult<()> {
            *self.live_lease.lock().expect("lease lock") = None;
            Ok(())
        }
        async fn retained_generations(
            &self,
            _scope: &engram_domain::Scope,
            _now: engram_domain::Timestamp,
            _grace_millis: u64,
        ) -> CoreResult<Vec<String>> {
            Ok(vec![self.generation.id.clone()])
        }
        async fn leased_generations(
            &self,
            _scope: &engram_domain::Scope,
            _now: engram_domain::Timestamp,
        ) -> CoreResult<Vec<String>> {
            // Test double: live-lease coverage mirrors the durable cell
            // behavior (proven against the real store in bound_source_status).
            Ok(self
                .live_lease
                .lock()
                .expect("lease lock")
                .as_ref()
                .map(|lease| lease.eligibility.generations.clone())
                .unwrap_or_default())
        }
    }

    #[async_trait::async_trait]
    impl CodeBindingRepository for InMemoryBindings {
        async fn register(
            &self,
            registration: engram_knowledge::RepositoryRegistration,
        ) -> CoreResult<engram_knowledge::RepositoryRegistration> {
            Ok(registration)
        }
        async fn bind(
            &self,
            binding: engram_domain::CodeBinding,
        ) -> CoreResult<engram_domain::CodeBinding> {
            Ok(binding)
        }
        async fn get_binding(
            &self,
            scope: &engram_domain::Scope,
            repo_id: &engram_domain::RepoId,
        ) -> CoreResult<Option<engram_domain::CodeBinding>> {
            Ok(
                (scope == &self.binding.scope && *repo_id == self.binding.repo_id)
                    .then(|| self.binding.clone()),
            )
        }
        async fn begin_attempt(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
            _generation: CodeGeneration,
            attempt: engram_domain::CodeAttempt,
        ) -> CoreResult<engram_domain::CodeAttempt> {
            Ok(attempt)
        }
        async fn get_attempt(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
            _attempt_id: &str,
        ) -> CoreResult<Option<(CodeGeneration, engram_domain::CodeAttempt)>> {
            Ok(None)
        }
        async fn get_generation(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
            generation_id: &str,
        ) -> CoreResult<Option<CodeGeneration>> {
            if generation_id == self.generation.id {
                return Ok(Some(self.generation.clone()));
            }
            if let Some(old) = &self.retained_old
                && old.id == generation_id
            {
                return Ok(Some(old.clone()));
            }
            Ok(None)
        }
        async fn target_lineage(
            &self,
            scope: &engram_domain::Scope,
        ) -> CoreResult<Vec<engram_knowledge::TargetLineage>> {
            if scope != &self.binding.scope {
                return Ok(Vec::new());
            }
            // Default: the single-chunk fixture participant. Mixed-generation
            // fixtures seed `extra_lineage` for their participating chunks.
            let mut lineage = vec![engram_knowledge::TargetLineage {
                record_id: "chunk-bound-1".to_owned(),
                repo_id: self.binding.repo_id.0.clone(),
                generation_id: self.generation.id.clone(),
            }];
            lineage.extend(self.extra_lineage.lock().expect("lineage lock").clone());
            Ok(lineage)
        }
        async fn get_source_descriptor(
            &self,
            scope: &engram_domain::Scope,
            repo_id: &engram_domain::RepoId,
            generation_id: &str,
            path: &str,
        ) -> CoreResult<Option<engram_domain::IndexedSource>> {
            if scope != &self.binding.scope
                || *repo_id != self.binding.repo_id
                || generation_id != self.generation.id
                || path != "src/bound.py"
            {
                return Ok(None);
            }
            Ok(Some(engram_domain::IndexedSource {
                repo_id: self.binding.repo_id.clone(),
                generation_id: self.generation.id.clone(),
                commit_oid: self.generation.commit_oid.clone(),
                path: path.to_owned(),
                content_digest: self
                    .descriptor_digest
                    .lock()
                    .expect("digest lock")
                    .clone()
                    .unwrap_or_default(),
            }))
        }
        async fn put_generation(&self, generation: CodeGeneration) -> CoreResult<CodeGeneration> {
            Ok(generation)
        }
        async fn put_lineage(
            &self,
            _scope: &engram_domain::Scope,
            _record_id: &str,
            lineage: engram_domain::CodeLineage,
        ) -> CoreResult<engram_domain::CodeLineage> {
            Ok(lineage)
        }
        async fn put_source_descriptor(
            &self,
            _scope: &engram_domain::Scope,
            source: engram_domain::IndexedSource,
        ) -> CoreResult<engram_domain::IndexedSource> {
            Ok(source)
        }
        async fn record_receipt(
            &self,
            receipt: engram_knowledge::StageReceipt,
        ) -> CoreResult<engram_knowledge::StageReceipt> {
            Ok(receipt)
        }
        async fn mark_ready(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
            _attempt_id: &str,
            _fencing_token: u64,
        ) -> CoreResult<engram_domain::CodeAttempt> {
            Err(CoreError::CapabilityUnsupported {
                capability: "binding-store".into(),
                reason: "test double is pre-published".into(),
            })
        }
        async fn publish(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
            _attempt_id: &str,
            _expected_epoch: u64,
        ) -> CoreResult<engram_domain::CodeBinding> {
            Ok(self.binding.clone())
        }
        async fn eligibility(
            &self,
            scope: &engram_domain::Scope,
        ) -> CoreResult<engram_knowledge::CodeEligibility> {
            let published = scope == &self.binding.scope;
            let mut generations = published
                .then(|| vec![self.generation.id.clone()])
                .unwrap_or_default();
            if let Some(old) = &self.retained_old {
                generations.push(old.id.clone());
            }
            Ok(engram_knowledge::CodeEligibility {
                scope: scope.clone(),
                epoch: u64::from(!published) + if published { self.binding.epoch } else { 0 },
                generations,
                repository_ids: published
                    .then(|| vec![self.binding.repo_id.0.clone()])
                    .unwrap_or_default(),
            })
        }
        async fn preview_reset(
            &self,
            _scope: &engram_domain::Scope,
            _repo_id: &engram_domain::RepoId,
        ) -> CoreResult<engram_domain::CodeResetPreview> {
            Err(CoreError::CapabilityUnsupported {
                capability: "code-reset".into(),
                reason: "test double holds no reset preview".into(),
            })
        }
        async fn journal_reset(
            &self,
            journal: engram_domain::CodeResetJournal,
        ) -> CoreResult<engram_domain::CodeResetJournal> {
            Ok(journal)
        }
    }

    /// An in-memory `KnowledgeQuery` double seeded with one bound code chunk
    /// stamped for gen-1 (task_017 MCP dispatch fixture). The neutral port
    /// keeps this file inside the ADR-0022 gated tree; the durable-store
    /// equivalent of this chain is proven in `bound_source_status`.
    struct SeededQuery {
        chunks: Vec<engram_domain::KnowledgeChunk>,
    }

    #[async_trait::async_trait]
    impl engram_integration::KnowledgeQuery for SeededQuery {
        async fn list_entities(
            &self,
            _scope: &engram_domain::Scope,
        ) -> CoreResult<Vec<engram_domain::KnowledgeEntity>> {
            Ok(Vec::new())
        }
        async fn list_relationships(
            &self,
            _scope: &engram_domain::Scope,
        ) -> CoreResult<Vec<engram_domain::KnowledgeRelationship>> {
            Ok(Vec::new())
        }
        async fn list_chunks(
            &self,
            _scope: &engram_domain::Scope,
        ) -> CoreResult<Vec<engram_domain::KnowledgeChunk>> {
            Ok(self.chunks.clone())
        }
    }

    fn bound_knowledge(
        scope: &engram_domain::Scope,
        source: &str,
    ) -> std::sync::Arc<dyn engram_integration::KnowledgeQuery> {
        bound_knowledge_impl(scope, source.as_bytes().to_vec(), "alpha", 1, 1)
    }

    fn bound_knowledge_impl(
        scope: &engram_domain::Scope,
        source: Vec<u8>,
        symbol_name: &str,
        start_line: u32,
        end_line: u32,
    ) -> std::sync::Arc<dyn engram_integration::KnowledgeQuery> {
        let mut stamps = engram_domain::Metadata::new();
        stamps.insert(
            "codeGenerationId".to_owned(),
            serde_json::Value::String("gen-1".to_owned()),
        );
        stamps.insert(
            "codeRepoId".to_owned(),
            serde_json::Value::String("bound-repo".to_owned()),
        );
        let chunk = engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from("chunk-bound-1"),
            document_id: engram_domain::Id::from("doc-bound"),
            source_id: engram_domain::Id::from("src-bound"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: String::from_utf8_lossy(&source)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: Some(start_line),
                end_line: Some(end_line),
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
            // Symbol selection input (finding 11): the chunk's entity name.
            entities: vec![engram_domain::EntityRef {
                id: Some(engram_domain::Id::from("entity-symbol")),
                kind: None,
                name: Some(symbol_name.to_owned()),
                aliases: Vec::new(),
            }],
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: "sha256:chunk-bound-1".to_owned(),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: Some(stamps),
        };
        std::sync::Arc::new(SeededQuery {
            chunks: vec![chunk],
        })
    }

    fn fixture_policy() -> engram_domain::Policy {
        engram_domain::Policy {
            visibility: engram_domain::Visibility::Workspace,
            retention: engram_domain::Retention::Durable,
            sensitivity: None,
            allowed_uses: vec![engram_domain::AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: None,
        }
    }

    fn fixture_provenance() -> engram_domain::Provenance {
        engram_domain::Provenance {
            source: "fetch-content-bound-test".to_owned(),
            actor: engram_domain::Actor {
                id: engram_domain::Id::from("fetch-content-bound-test"),
                kind: engram_domain::ActorKind::Agent,
                display_name: None,
                metadata: None,
            },
            observed_at: chrono::Utc::now(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("test".to_owned()),
        }
    }

    /// Builds a BOUND MCP app over a real retained snapshot: repo bound +
    /// published at `gen-1` whose commit OID owns the retained bytes, with
    /// knowledge chunks stamped for that generation. Returns the app plus the
    /// retained root for deletion experiments.
    fn bound_app(dir: &std::path::Path, source: &str) -> (App, std::path::PathBuf) {
        bound_app_impl(dir, source.as_bytes().to_vec(), "alpha", 1, 1)
    }

    /// `bound_app` with a symbol-carrying chunk spanning [start_line, end_line].
    fn bound_app_with_symbols(
        dir: &std::path::Path,
        source: &str,
        start_line: u32,
        end_line: u32,
    ) -> (App, std::path::PathBuf) {
        bound_app_with_symbol(dir, source, "alpha", start_line, end_line)
    }

    /// `bound_app` with an explicit symbol name on the chunk.
    fn bound_app_with_symbol(
        dir: &std::path::Path,
        source: &str,
        symbol: &str,
        start_line: u32,
        end_line: u32,
    ) -> (App, std::path::PathBuf) {
        bound_app_impl(
            dir,
            source.as_bytes().to_vec(),
            symbol,
            start_line,
            end_line,
        )
    }

    /// `bound_app` over raw (possibly non-UTF-8) bytes.
    fn bound_app_with_symbols_bytes(
        dir: &std::path::Path,
        source: &[u8],
        start_line: u32,
        end_line: u32,
    ) -> (App, std::path::PathBuf) {
        bound_app_impl(dir, source.to_vec(), "alpha", start_line, end_line)
    }

    /// `bound_app` whose chunk set contains TWO same-named entities with
    /// different spans (ambiguous symbol).
    fn bound_app_with_ambiguous_symbols(
        dir: &std::path::Path,
        source: &str,
    ) -> (App, std::path::PathBuf) {
        bound_app_impl_ambiguous(dir, source.as_bytes().to_vec())
    }

    fn bound_app_impl(
        dir: &std::path::Path,
        source: Vec<u8>,
        symbol_name: &str,
        start_line: u32,
        end_line: u32,
    ) -> (App, std::path::PathBuf) {
        let scope = crate::scope::project_scope("bound-project", "default");
        let commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: "a".repeat(40),
            object_format: "sha1".to_owned(),
        };
        let files = vec![CommittedFile {
            path: "src/bound.py".into(),
            oid: "b".repeat(40),
            mode: "100644".to_owned(),
            bytes: source.clone(),
        }];
        let retained_root = dir.join("retained");
        let snapshot = retain_snapshot(&retained_root, commit, &files).expect("retain snapshot");
        let bindings = std::sync::Arc::new(InMemoryBindings::published(&snapshot.commit.oid));
        bindings.set_descriptor_digest(
            engram_ingest::content_hash(&source)
                .trim_start_matches("sha256:")
                .to_string(),
        );
        let knowledge =
            bound_knowledge_impl(&scope, source.clone(), symbol_name, start_line, end_line);
        let coordinator = CodeBindingCoordinator::new(
            bindings.clone() as std::sync::Arc<dyn CodeBindingRepository>,
            std::sync::Arc::new(PermissiveAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            engram_domain::CapabilityState::Supported,
        ))
        .knowledge_query(knowledge)
        .code_bindings(std::sync::Arc::new(coordinator))
        .code_source(std::sync::Arc::new(RetainedSnapshotReader::new(
            retained_root.clone(),
        )))
        .build();
        (
            App {
                provider,
                scope,
                ontology: OntologyConfig::default(),
                taxonomy: TaxonomyConfig::default(),
                storage_dir: dir.to_path_buf(),
            },
            retained_root,
        )
    }

    fn bound_app_impl_ambiguous(
        dir: &std::path::Path,
        source: Vec<u8>,
    ) -> (App, std::path::PathBuf) {
        let scope = crate::scope::project_scope("bound-project", "default");
        let commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: "a".repeat(40),
            object_format: "sha1".to_owned(),
        };
        let files = vec![CommittedFile {
            path: "src/bound.py".into(),
            oid: "b".repeat(40),
            mode: "100644".to_owned(),
            bytes: source.clone(),
        }];
        let retained_root = dir.join("retained2");
        let snapshot = retain_snapshot(&retained_root, commit, &files).expect("retain");
        let bindings = std::sync::Arc::new(InMemoryBindings::published(&snapshot.commit.oid));
        bindings.set_descriptor_digest(
            engram_ingest::content_hash(&source)
                .trim_start_matches("sha256:")
                .to_string(),
        );
        let knowledge = ambiguous_knowledge(&scope);
        // Both ambiguous chunks carry durable lineage on the published
        // generation — canonical participants whose spans genuinely clash.
        bindings.with_chunk_lineage(&[
            ("chunk-alpha-1".to_owned(), "gen-1".to_owned()),
            ("chunk-alpha-2".to_owned(), "gen-1".to_owned()),
        ]);
        let coordinator = CodeBindingCoordinator::new(
            bindings.clone() as std::sync::Arc<dyn CodeBindingRepository>,
            std::sync::Arc::new(PermissiveAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            engram_domain::CapabilityState::Supported,
        ))
        .knowledge_query(knowledge)
        .code_bindings(std::sync::Arc::new(coordinator))
        .code_source(std::sync::Arc::new(RetainedSnapshotReader::new(
            retained_root.clone(),
        )))
        .build();
        (
            App {
                provider,
                scope,
                ontology: OntologyConfig::default(),
                taxonomy: TaxonomyConfig::default(),
                storage_dir: dir.to_path_buf(),
            },
            retained_root,
        )
    }

    /// Two same-named symbol chunks with DIFFERENT spans (ambiguous).
    fn ambiguous_knowledge(
        scope: &engram_domain::Scope,
    ) -> std::sync::Arc<dyn engram_integration::KnowledgeQuery> {
        let chunk = |id: &str, start: u32, end: u32| engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from(id),
            document_id: engram_domain::Id::from("doc-bound"),
            source_id: engram_domain::Id::from("src-bound"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: "def alpha():".to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: Some(start),
                end_line: Some(end),
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
            entities: vec![engram_domain::EntityRef {
                id: Some(engram_domain::Id::from(id)),
                kind: None,
                name: Some("alpha".to_owned()),
                aliases: Vec::new(),
            }],
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: format!("sha256:{id}"),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: {
                let mut stamps = engram_domain::Metadata::new();
                stamps.insert(
                    "codeGenerationId".to_owned(),
                    serde_json::Value::String("gen-1".to_owned()),
                );
                stamps.insert(
                    "codeRepoId".to_owned(),
                    serde_json::Value::String("bound-repo".to_owned()),
                );
                Some(stamps)
            },
        };
        std::sync::Arc::new(SeededQuery {
            chunks: vec![chunk("chunk-alpha-1", 1, 2), chunk("chunk-alpha-2", 4, 5)],
        })
    }

    /// Corrective r2 (finding 11): symbol selection serves the SAME-GENERATION
    /// byte span — only beta's lines, byte-exact with CRLF preserved — and a
    /// MISSING symbol denies.
    #[test]
    fn fetch_bound_symbol_span_is_exact_and_missing_symbol_denies() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n";
        let (app, _retained) = bound_app_with_symbol(dir.path(), source, "beta", 4, 5);
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "beta" })).unwrap(),
        );
        assert_eq!(out["source"], "exact");
        assert_eq!(
            out["span"]["start"],
            json!(source.find("def beta").unwrap())
        );
        let content = out["content"].as_str().unwrap();
        assert_eq!(
            content, "def beta():\r\n    return 2\r\n",
            "byte-exact symbol span"
        );
        assert_ne!(out["content_sha256_full"], out["content_sha256"]);

        let err = fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "nosuch" }))
            .unwrap_err();
        assert!(
            err.message.contains("symbol selection failed"),
            "missing symbols deny: {}",
            err.message
        );
    }

    /// Mixed-generation fixture (r3): OLD (gen-1) and CURRENT (gen-2)
    /// retained chunks coexist for one path. In gen-1 `alpha` sat at lines
    /// 1-2 and `gamma` existed; in gen-2 `beta` was added at 2-3 and `alpha`
    /// MOVED to 6-7 (and `gamma` no longer exists).
    fn mixed_generation_app(dir: &std::path::Path) -> (App, std::path::PathBuf, String, String) {
        let scope = crate::scope::project_scope("bound-project", "default");
        let old_source =
            b"def alpha():\r\n    return 1\r\n\r\ndef gamma():\r\n    return 9\r\n".to_vec();
        let current_source =
            b"import os\r\ndef beta():\r\n    return 2\r\n\r\n\r\ndef alpha():\r\n    return 3\r\n"
                .to_vec();
        let old_commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: "e".repeat(40),
            object_format: "sha1".to_owned(),
        };
        let current_commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: "a".repeat(40),
            object_format: "sha1".to_owned(),
        };
        let retained_root = dir.join("retained-mixed");
        retain_snapshot(
            &retained_root,
            old_commit.clone(),
            &[CommittedFile {
                path: "src/bound.py".into(),
                oid: "f".repeat(40),
                mode: "100644".to_owned(),
                bytes: old_source.clone(),
            }],
        )
        .expect("retain old");
        retain_snapshot(
            &retained_root,
            current_commit.clone(),
            &[CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: current_source.clone(),
            }],
        )
        .expect("retain current");

        let old_oid = old_commit.oid.clone();
        let current_oid = current_commit.oid.clone();
        let bindings = std::sync::Arc::new(InMemoryBindings::published_two_generations(
            &old_oid,
            &current_oid,
        ));
        bindings.set_descriptor_digest(
            engram_ingest::content_hash(&current_source)
                .trim_start_matches("sha256:")
                .to_string(),
        );
        // Chunks of BOTH generations for one path (retention coexistence).
        let stamp = |generation: &str| {
            let mut metadata = engram_domain::Metadata::new();
            metadata.insert(
                "codeGenerationId".to_owned(),
                serde_json::Value::String(generation.to_owned()),
            );
            metadata.insert(
                "codeRepoId".to_owned(),
                serde_json::Value::String("bound-repo".to_owned()),
            );
            metadata
        };
        let chunk_of = |id: &str,
                        generation: &str,
                        symbol: &str,
                        start: u32,
                        end: u32,
                        first_line: &str|
         -> engram_domain::KnowledgeChunk {
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from(id),
                document_id: engram_domain::Id::from("doc-bound-mixed"),
                source_id: engram_domain::Id::from("src-bound-mixed"),
                kind: KnowledgeChunkKind::CodeSymbol,
                text: first_line.to_owned(),
                summary: None,
                location: Some(engram_domain::SourceLocation {
                    path: Some("src/bound.py".to_owned()),
                    start_line: Some(start),
                    end_line: Some(end),
                    start_offset: None,
                    end_offset: None,
                    anchor: None,
                }),
                entities: vec![engram_domain::EntityRef {
                    id: Some(engram_domain::Id::from(id)),
                    kind: None,
                    name: Some(symbol.to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: format!("sha256:{id}"),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: Some(stamp(generation)),
            }
        };
        let mut chunks = vec![
            // gen-1 (retained, superseded)
            chunk_of("chunk-old-alpha", "gen-1", "alpha", 1, 2, "def alpha():"),
            chunk_of("chunk-old-gamma", "gen-1", "gamma", 4, 5, "def gamma():"),
            // gen-2 (current published)
            chunk_of("chunk-new-beta", "gen-2", "beta", 2, 3, "def beta():"),
            chunk_of("chunk-new-alpha", "gen-2", "alpha", 6, 7, "def alpha():"),
        ];
        // IMPOSTORS (canonical 10): an UNSTAMPED row and a FORGED-CURRENT-
        // stamp row, both naming `alpha` at WRONG lines with NO durable
        // lineage — neither may select or ambiguate the span.
        let mut unstamped = chunk_of(
            "chunk-impostor-unstamped",
            "gen-none",
            "alpha",
            1,
            1,
            "def alpha():",
        );
        unstamped.metadata = None;
        chunks.push(unstamped);
        chunks.push(chunk_of(
            "chunk-impostor-forged",
            "gen-2",
            "alpha",
            3,
            3,
            "def alpha():",
        ));
        // Canonical durable lineage: the OLD chunks belong to the retired
        // gen-1 (not participants) and the CURRENT chunks to gen-2; the
        // unstamped and forged-stamp IMPOSTORS deliberately carry NO
        // lineage — stamps are not admissible evidence.
        bindings.with_chunk_lineage(&[
            ("chunk-old-alpha".to_owned(), "gen-1".to_owned()),
            ("chunk-old-gamma".to_owned(), "gen-1".to_owned()),
            ("chunk-new-beta".to_owned(), "gen-2".to_owned()),
            ("chunk-new-alpha".to_owned(), "gen-2".to_owned()),
        ]);
        let knowledge: std::sync::Arc<dyn engram_integration::KnowledgeQuery> =
            std::sync::Arc::new(SeededQuery { chunks });
        let coordinator = CodeBindingCoordinator::new(
            bindings.clone() as std::sync::Arc<dyn CodeBindingRepository>,
            std::sync::Arc::new(PermissiveAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            engram_domain::CapabilityState::Supported,
        ))
        .knowledge_query(knowledge)
        .code_bindings(std::sync::Arc::new(coordinator))
        .code_source(std::sync::Arc::new(RetainedSnapshotReader::new(
            retained_root.clone(),
        )))
        .build();
        (
            App {
                provider,
                scope,
                ontology: OntologyConfig::default(),
                taxonomy: TaxonomyConfig::default(),
                storage_dir: dir.to_path_buf(),
            },
            retained_root,
            current_oid,
            current_source.len().to_string(),
        )
    }

    /// Mixed retained generations resolve the CURRENT authorized generation:
    /// the served revision is the current commit and the symbol span comes
    /// from the CURRENT generation's chunks (a moved symbol serves its NEW
    /// span, never the old generation's).
    #[test]
    fn mixed_generations_resolve_current_origin_and_spans() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _retained, current_oid, _len) = mixed_generation_app(dir.path());
        let current_source =
            b"import os\r\ndef beta():\r\n    return 2\r\n\r\n\r\ndef alpha():\r\n    return 3\r\n";

        // `alpha` exists in BOTH generations at DIFFERENT lines: the answer
        // must come from the CURRENT generation (moved span).
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" })).unwrap(),
        );
        assert_eq!(out["source"], "exact");
        assert_eq!(out["generation"], "gen-2");
        assert_eq!(out["revision"], current_oid);
        let expected_start = current_source
            .windows(b"def alpha".len())
            .position(|w| w == b"def alpha")
            .expect("current alpha offset");
        assert_eq!(out["span"]["start"], json!(expected_start));
        let content = out["content"].as_str().unwrap();
        assert_eq!(content, "def alpha():\r\n    return 3\r\n");

        // A symbol that exists ONLY in the current generation works.
        let beta = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "beta" })).unwrap(),
        );
        assert_eq!(beta["generation"], "gen-2");
        let beta_content = beta["content"].as_str().unwrap();
        assert_eq!(beta_content, "def beta():\r\n    return 2\r\n");
    }

    /// Corrective r4 (canonical 10, fallback consumption): in explicit
    /// `chunks` mode the bound path's reconstruction consumes ONLY the
    /// canonical current-generation participants — the old-generation and
    /// impostor chunks can never inject fallback content, and the line span
    /// reports the participants' span.
    #[test]
    fn bound_chunks_fallback_consumes_only_canonical_participants() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _retained, current_oid, _len) = mixed_generation_app(dir.path());
        let _ = current_oid;
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
        );
        assert_eq!(
            out["chunk_count"],
            json!(2),
            "only the two current-generation participants: {out}"
        );
        assert_eq!(out["lines"]["first"], json!(2), "participants' span: {out}");
        assert_eq!(out["lines"]["last"], json!(7), "participants' span: {out}");
        let content = out["content"].as_str().unwrap();
        for forbidden in ["def gamma", "impostor"] {
            assert!(
                !content.contains(forbidden),
                "old-generation/impostor chunks must not inject fallback content: {content}"
            );
        }
        assert!(
            content.contains("def beta") && content.contains("def alpha"),
            "the current generation's participants supply the fallback: {content}"
        );
    }

    /// An OLD-ONLY symbol (present in the superseded generation's chunks,
    /// absent from the current generation) denies — the old retained chunks
    /// never win symbol selection.
    #[test]
    fn mixed_generations_old_only_symbol_denies() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _retained, _oid, _len) = mixed_generation_app(dir.path());
        let err =
            fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "gamma" })).unwrap_err();
        assert!(
            err.message.contains("symbol selection failed")
                && err.message.contains("no chunk in the indexed generation"),
            "old-only symbols must deny against the current generation: {}",
            err.message
        );
    }

    /// Corrective r2 (finding 11): an AMBIGUOUS symbol (two same-named
    /// entities with different spans in the same generation) denies instead
    /// of silently picking one.
    #[test]
    fn fetch_bound_ambiguous_symbol_denies() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\r\n    return 1\r\n\r\ndef alpha():\r\n    return 2\r\n";
        let (app, _retained) = bound_app_with_ambiguous_symbols(dir.path(), source);
        let err =
            fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" })).unwrap_err();
        assert!(
            err.message.contains("ambiguous"),
            "ambiguous symbols deny: {}",
            err.message
        );
    }

    /// Corrective r2 (finding 11): a byte cap at a MULTIBYTE boundary keeps
    /// the response byte-exact (hex encoding for the invalid slice) and
    /// `content_sha256` hashes the RETURNED bytes, not the full blob.
    #[test]
    fn fetch_bound_multibyte_cap_hashes_returned_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\r\n    return '\u{e9}\u{e9}\u{e9}'\r\n";
        let (app, _retained) = bound_app_with_symbols(dir.path(), source, 1, 1);
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "max_bytes": 27 })).unwrap(),
        );
        assert_eq!(out["truncated"], true);
        assert_eq!(out["returned_bytes"], json!(27), "the byte cap is honored");
        assert_eq!(out["total_bytes"], json!(source.len()));
        let returned = &source.as_bytes()[..27];
        let expected = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(returned);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(out["content_sha256"], json!(expected));
        assert_ne!(out["content_sha256"], out["content_sha256_full"]);
        assert_eq!(
            out["encoding"], "hex",
            "a mid-character cap is hex-encoded, never lossy"
        );
    }

    /// Corrective r2 (finding 11): NON-UTF-8 retained bytes return an
    /// explicitly labeled hex encoding — no lossy exactness claims.
    #[test]
    fn fetch_bound_non_utf8_bytes_return_labeled_hex() {
        let dir = tempfile::tempdir().unwrap();
        let source_bytes: Vec<u8> = b"def alpha():\r\n    x = '"
            .to_vec()
            .into_iter()
            .chain([0x80, 0x81, 0x82])
            .chain(b"'\r\n".to_vec())
            .collect();
        let lossy = String::from_utf8_lossy(&source_bytes).into_owned();
        let (app, _retained) = bound_app_with_symbols_bytes(dir.path(), &source_bytes, 1, 3);
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
        assert_eq!(out["source"], "exact");
        assert_eq!(out["encoding"], "hex");
        let hex = out["content"].as_str().unwrap();
        assert_eq!(
            hex.len(),
            source_bytes.len() * 2,
            "exact bytes, hex-encoded"
        );
        let expected = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&source_bytes);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(out["content_sha256"], json!(expected));
        assert_ne!(
            hex, lossy,
            "never lossily decoded to the replacement string"
        );
    }

    /// Bound code serves byte-exact RETAINED bytes with revision labels even
    /// after the working tree is deleted (task_017: no checkout dependency).
    #[test]
    fn fetch_bound_code_serves_exact_retained_bytes_after_checkout_deleted() {
        let dir = tempfile::tempdir().unwrap();
        // CRLF content: byte-exactness must preserve it.
        let source = "def alpha():\r\n    return 1\r\n\u{00e9}t\u{00e9} marker\r\n";
        let (app, _retained) = bound_app(dir.path(), source);
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
        assert_eq!(out["source"], "exact");
        assert_eq!(out["revision"], "a".repeat(40));
        let content = out["content"].as_str().unwrap();
        assert_eq!(
            content, source,
            "byte-exact retained bytes (CRLF preserved)"
        );
        assert!(
            out["note"]
                .as_str()
                .unwrap()
                .contains("no checkout fallback")
        );
    }

    /// A missing retained snapshot is an ERROR for bound code — a decoy file
    /// on disk with different bytes is never served as the revision.
    #[test]
    fn fetch_bound_code_missing_snapshot_errors_never_disk_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\n    return 1\n";
        let (app, retained_root) = bound_app(dir.path(), source);
        // Delete the retained snapshot; plant a DECOY working-tree file.
        std::fs::remove_dir_all(&retained_root).unwrap();
        let decoy_dir = dir.path().join("decoy");
        std::fs::create_dir_all(decoy_dir.join("src")).unwrap();
        std::fs::write(decoy_dir.join("src/bound.py"), "IMPOSTER CONTENT").unwrap();
        let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            err.message.contains("ExactSourceUnavailable"),
            "missing snapshot must surface the typed unavailable error: {}",
            err.message
        );
    }

    /// mode=chunks on a bound path is an honest reconstruction — labeled, and
    /// NEVER presented as exact source.
    #[test]
    fn fetch_bound_code_chunks_mode_labels_reconstruction() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\n    return 1\n";
        let (app, _retained) = bound_app(dir.path(), source);
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
        );
        assert_eq!(out["source"], "chunks");
        assert_ne!(out["source"], "exact");
        assert!(
            out["note"].as_str().unwrap().contains("best-effort"),
            "chunk mode must stay labeled as best-effort reconstruction: {}",
            out["note"]
        );
    }

    /// EN-14/EN-16 (eval 0.2.5): default fetch returns VERBATIM on-disk
    /// source — a function containing a nested declaration comes back whole
    /// (container chunks alone reconstruct signature only), and the whole
    /// file is byte-exact.
    #[test]
    fn fetch_defaults_to_verbatim_source() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        let source = "def outer():\n    def inner():\n        return 1\n    before = 2\n    after = inner()\n    return before + after\n";
        std::fs::write(repo.path().join("src/mod.py"), source).unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/mod.py" })).unwrap());
        assert_eq!(out["source"], "verbatim");
        let content = out["content"].as_str().unwrap();
        assert_eq!(content, source, "byte-exact verbatim source");
        assert!(out["truncated"] == false);
    }

    /// EN-16: symbol-scoped fetch — one function's span, not the whole file.
    #[test]
    fn fetch_symbol_slice_returns_just_that_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(
            repo.path().join("src/mod.py"),
            "def alpha():\n    return 1\n\ndef beta():\n    return 2\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/mod.py", "symbol": "beta" })).unwrap(),
        );
        let content = out["content"].as_str().unwrap();
        assert!(content.contains("def beta"), "slice: {content}");
        assert!(
            !content.contains("def alpha"),
            "slice must exclude alpha: {content}"
        );
        assert_eq!(out["lines"]["first"], 4);
    }

    /// mode=chunks keeps the legacy reconstruction (with its honest note).
    #[test]
    fn fetch_chunks_mode_is_explicit_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("m.py"), "def f():\n    return 1\n").unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out =
            content_of(&fetch_content(&app, &json!({ "path": "m.py", "mode": "chunks" })).unwrap());
        assert_eq!(out["source"], "chunks");
        assert!(
            out["note"].as_str().unwrap().contains("best-effort"),
            "legacy note: {}",
            out["note"]
        );
    }

    /// mode=verbatim with an unresolvable root errors instead of silently
    /// falling back.
    #[test]
    fn fetch_verbatim_mode_errors_when_unresolvable() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("m.py"), "def f():\n    return 1\n").unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        // Drop the recorded roots (simulate a foreign store with no registry).
        let _ = std::fs::remove_file(dir.path().join("scan-roots.json"));
        crate::scan_roots::roots_for_tests_remove(dir.path(), "engram-mcp-scan");
        let err = fetch_content(&app, &json!({ "path": "m.py", "mode": "verbatim" })).unwrap_err();
        assert!(err.message.contains("verbatim read unavailable"));
    }

    #[test]
    fn fetches_stitched_markdown_doc() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("guide.md"),
            "# Guide\n\nAlpha section body.\n\n## Beta\n\nBeta section body.\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out = content_of(&fetch_content(&app, &json!({ "path": "guide.md" })).unwrap());
        assert_eq!(out["kind"], "doc");
        assert_eq!(out["truncated"], false);
        let content = out["content"].as_str().unwrap();
        assert!(content.contains("# Guide"), "content: {content}");
        assert!(
            content.contains("Alpha section body."),
            "content: {content}"
        );
        assert!(content.contains("Beta section body."), "content: {content}");
    }

    #[test]
    fn fetches_stitched_code_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn alpha() {}\n\npub fn beta() {}\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/lib.rs" })).unwrap());
        assert_eq!(out["kind"], "code");
        assert!(out["chunk_count"].as_u64().unwrap() >= 2, "out: {out}");
        let content = out["content"].as_str().unwrap();
        assert!(content.contains("pub fn alpha()"), "content: {content}");
        assert!(content.contains("pub fn beta()"), "content: {content}");
        assert!(
            out["note"].is_string(),
            "code responses carry the lossy note"
        );
    }

    #[test]
    fn max_bytes_cap_truncates_on_char_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("big.md"),
            "# T\n\nαααααααααα body text beyond the cap.\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "big.md", "max_bytes": 12 })).unwrap(),
        );
        assert_eq!(out["truncated"], true);
        assert!(out["total_bytes"].as_u64().unwrap() > 12);
        assert!(out["content"].as_str().unwrap().len() <= 12);
    }

    #[test]
    fn unknown_path_is_an_error_with_hint() {
        let dir = tempfile::tempdir().unwrap();
        let app = test_app(dir.path());
        let err = fetch_content(&app, &json!({ "path": "nope.md" })).unwrap_err();
        let msg = err.message.clone();
        assert!(msg.contains("no indexed chunks"), "msg: {msg}");
        assert!(msg.contains("scan_repo"), "msg: {msg}");
    }

    #[test]
    fn same_path_from_two_sources_is_ambiguous_until_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("docs")).unwrap();
        std::fs::write(
            repo.path().join("docs/guide.md"),
            "# Guide\n\nScanned body.\n",
        )
        .unwrap();
        let app = test_app(dir.path());
        scan(&app, repo.path());
        // Re-index the same path through the docs lane — different provenance
        // source (`mcp-index-docs` vs the scan's `engram-mcp-scan`).
        crate::tools::index_docs(
            &app,
            &json!({ "path": "docs/guide.md", "content": "# Guide\n\nRe-indexed body.\n" }),
        )
        .unwrap();

        let out = content_of(&fetch_content(&app, &json!({ "path": "docs/guide.md" })).unwrap());
        assert_eq!(out["status"], "ambiguous", "out: {out}");
        assert_eq!(out["candidates"].as_array().unwrap().len(), 2);

        // Repository filter resolves it: use the non-scan candidate value
        // exactly as the candidates list reported it.
        let repo_filter = out["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["repository"].as_str().unwrap())
            .find(|r| !r.contains("engram-mcp-scan"))
            .expect("non-scan candidate")
            .to_owned();
        let resolved = content_of(
            &fetch_content(
                &app,
                &json!({ "path": "docs/guide.md", "repository": repo_filter }),
            )
            .unwrap(),
        );
        let content = resolved["content"].as_str().unwrap();
        assert!(content.contains("Re-indexed body."), "resolved: {content}");
    }

    #[test]
    fn truncate_bytes_respects_char_boundaries() {
        let (cut, truncated) = truncate_bytes("αβγδ", 3);
        assert!(truncated);
        assert_eq!(cut, "α"); // 3 bytes cuts back to the 2-byte boundary
        let (cut, truncated) = truncate_bytes("αβγδ", 4);
        assert!(truncated);
        assert_eq!(cut, "αβ"); // exactly two 2-byte chars
        let (cut, truncated) = truncate_bytes("αβγδ", 6);
        assert!(truncated);
        assert_eq!(cut, "αβγ");
        let (cut, truncated) = truncate_bytes("αβγδ", 100);
        assert!(!truncated);
        assert_eq!(cut, "αβγδ");
    }
}
