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

// clippy roll 2026-10: test fixture helpers and tuple input preserve existing transport cases.
#![allow(dead_code)]
#![allow(clippy::type_complexity)]
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

/// Canonical participating chunks (corrective r5, A4): one immutable
/// input snapshot per matched row for the SAME-OPERATION exact read. The
/// authoritative participant set, origin, spans and bytes are resolved
/// inside the neutral coordinator/provider operation (`code_fetch_exact_participants`)
/// — this helper only shapes the caller-side inputs (ids, claimed spans,
/// symbol names, first stored line) from the listing that matched the path.
fn exact_read_candidates(
    matched: &[&engram_domain::KnowledgeChunk],
) -> Vec<engram_integration::code_binding::ExactReadCandidate> {
    matched
        .iter()
        .map(|chunk| {
            let location = chunk.location.as_ref();
            let mut names: Vec<String> = chunk
                .entities
                .iter()
                .filter_map(|entity| entity.name.clone())
                .collect();
            if let Some(anchor) = location.and_then(|location| location.anchor.as_deref()) {
                names.push(anchor.to_owned());
            }
            engram_integration::code_binding::ExactReadCandidate {
                record_id: chunk.id.to_string(),
                start_line: location.and_then(|location| location.start_line),
                end_line: location.and_then(|location| location.end_line),
                names,
                text_head: chunk.text.lines().next().unwrap_or("").to_owned(),
            }
        })
        .collect()
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

    // Kind: any symbol/code chunk => code; otherwise document chunks => doc.
    let is_code = ordered.iter().any(|c| {
        matches!(
            c.kind,
            KnowledgeChunkKind::CodeSymbol | KnowledgeChunkKind::CodeBlock
        )
    });
    let kind = if is_code { "code" } else { "doc" };

    // --- Bound code: ONE same-operation exact read (task_017 / A4) ---------
    // Participant identity (durable lineage vs the published eligible
    // bindings — never metadata stamps), the origin repository/generation,
    // caller authority, the retained descriptor, the digest-validated
    // bytes, the symbol span AND the span-content validation are resolved
    // INSIDE one consistent-read window by the neutral coordinator
    // operation (a publication racing the operation retries once against
    // the rebuilt view or denies — never mixed-generation output). A
    // canonical-ID row whose claimed path or span was changed fails the
    // in-window content validation against the digest-authenticated bytes.
    let actor = engram_domain::Actor {
        id: engram_domain::Id::from("engram-mcp-fetch-content"),
        kind: engram_domain::ActorKind::Agent,
        display_name: None,
        metadata: None,
    };
    let outcome = futures::executor::block_on(app.provider.code_fetch_exact_participants(
        &actor,
        &app.scope,
        &path,
        &exact_read_candidates(&ordered),
        symbol,
    ));
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(ToolError::new(
                -32603,
                format!("exact retained source unavailable: {error}"),
            ));
        }
    };
    match outcome {
        engram_integration::code_binding::ParticipantExactSource::Denied {
            symbol_selection,
            reason,
        } => {
            return Err(ToolError::new(
                -32602,
                if symbol_selection {
                    format!("symbol selection failed: {reason}")
                } else {
                    reason
                },
            ));
        }
        engram_integration::code_binding::ParticipantExactSource::Served {
            repo_id,
            generation_id,
            view: full_view,
            span,
            participants,
        } => {
            if mode == "chunks" {
                // Final r5 C4 (verification finding 1): the bound path's
                // best-effort reconstruction emits ONLY authenticated
                // retained bytes — each participant's DURABLE canonical
                // span sliced out of the digest-validated revision blob.
                // The mutable stored chunk text is NEVER consumed for
                // bound code output: a current participant whose stored
                // text was substituted (the retired neighbor's text,
                // another file's content) rides through nothing — a
                // validated participant ID is not a validated payload, so
                // only the span's authenticated bytes can be served.
                // Ordering/completeness match the stored-text stitch for
                // legitimate data: every admitted participant's claimed
                // span EQUALS its durable span, so durable-span order IS
                // claim order, and the participant set is exactly the set
                // the old ID filter would have admitted.
                let mut participants = participants;
                participants
                    .sort_by_key(|participant| (participant.start_line, participant.end_line));
                let mut parts: Vec<&str> = Vec::with_capacity(participants.len());
                for participant in &participants {
                    let (start, end) = participant.byte_span;
                    let text = std::str::from_utf8(&full_view.bytes[start..end]).map_err(|_| {
                        ToolError::new(
                            -32602,
                            format!(
                                "bound chunks reconstruction unavailable: participant \
                                     '{}' has a span that is not valid UTF-8 in the retained \
                                     revision — use the default exact read (it returns the \
                                     exact bytes, labeled)",
                                participant.record_id
                            ),
                        )
                    })?;
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
                // The repository label comes from a PARTICIPANT row (the
                // rows whose authenticated bytes are served) — never from
                // a non-participant row that merely matched the path.
                let source = participants
                    .first()
                    .and_then(|participant| {
                        ordered
                            .iter()
                            .find(|chunk| chunk.id.as_str() == participant.record_id)
                    })
                    .map(|chunk| chunk.provenance.source.as_str())
                    .unwrap_or_default();
                return stitch_parts(
                    &path,
                    source,
                    kind,
                    is_code,
                    &parts,
                    participants.len(),
                    participants
                        .first()
                        .map(|participant| participant.start_line),
                    participants.last().map(|participant| participant.end_line),
                    max_bytes,
                    verbatim_fallback_note,
                );
            }
            // Corrective r2 (finding 11): the exact path honors SYMBOL
            // selection from the SAME generation (missing/ambiguous symbols
            // deny), slices the retained bytes at same-OID byte offsets, and
            // reports truthful digests: `content_sha256_full` hashes the
            // whole retained blob and `content_sha256` hashes the bytes
            // ACTUALLY RETURNED after caps. Non-UTF-8 bytes are never lossily
            // decoded: they are returned as an explicitly labeled hex
            // encoding of the exact bytes.
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
            // Byte-exact cap: truncate on the returned BYTE budget; UTF-8
            // safety is handled by the encoding branch below (no lossy
            // replacement).
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
                    // Explicit non-UTF-8 handling: hex-encode the exact
                    // bytes — no lossy replacement that would change them.
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
        engram_integration::code_binding::ParticipantExactSource::Unbound => {}
    }
    // `Unbound`: no matched row carries DURABLE lineage — the ordinary
    // paths apply. A row that CLAIMS bound code through metadata stamps
    // without any durable lineage is an impostor's claim: deny rather than
    // serve it as ordinary content. (The stamp check is ONLY this backstop
    // for forged claims — the bound/ordinary classification itself is the
    // lineage-based one resolved above, and old-generation lineage denied
    // inside the operation.)
    let has_bound_claim = ordered.iter().any(|chunk| {
        chunk.metadata.as_ref().is_some_and(|metadata| {
            metadata.contains_key("codeRepoId") || metadata.contains_key("codeGenerationId")
        })
    });
    if has_bound_claim {
        return Err(ToolError::new(
            -32602,
            "bound source has no current canonical participants",
        ));
    }

    // Disambiguate multi-repo scopes: same rel path indexed from several
    // sources. Decided AFTER canonical participant filtering — a
    // different-source impostor beside a legitimate current participant
    // cannot force an ambiguous response for a bound path (the bound answer
    // above already won). Surface the candidates instead of silently mixing
    // content.
    let sources: Vec<&str> = {
        let mut s: Vec<&str> = ordered
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
                let count = ordered.iter().filter(|c| c.provenance.source == *s).count();
                json!({ "repository": s, "chunk_count": count })
            })
            .collect();
        return Ok(protocol::text_content(
            json!({
                "status": "ambiguous",
                "path": path,
                "message": "path indexed from multiple sources — pass the exact `repository` value of one candidate (or the search-style \"org/name\" for git-backed sources)",
                "candidates": candidates,
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

    // Unbound path: the historical stitching over the matched rows (a bound
    // path never reaches here — it stitched only its validated participants
    // above or denied).
    stitch_chunks(
        &path,
        sources[0],
        kind,
        is_code,
        &ordered,
        max_bytes,
        verbatim_fallback_note,
    )
}

/// The legacy best-effort reconstruction: skip graph-only anchors (empty
/// text — containers with neither doc comment nor signature); docs join
/// with a blank line (section boundaries); code joins with a single newline
/// (line spans are contiguous-ish). Always labeled as best-effort.
fn stitch_chunks(
    path: &str,
    source: &str,
    kind: &str,
    is_code: bool,
    fallback_chunks: &[&engram_domain::KnowledgeChunk],
    max_bytes: usize,
    verbatim_fallback_note: Option<String>,
) -> Result<Value, ToolError> {
    let parts: Vec<&str> = fallback_chunks
        .iter()
        .filter(|c| !c.text.is_empty())
        .map(|c| c.text.as_str())
        .collect();
    let first_line = fallback_chunks
        .iter()
        .find_map(|c| c.location.as_ref().and_then(|l| l.start_line));
    let last_line = fallback_chunks
        .iter()
        .rev()
        .find_map(|c| c.location.as_ref().and_then(|l| l.end_line));
    stitch_parts(
        path,
        source,
        kind,
        is_code,
        &parts,
        fallback_chunks.len(),
        first_line,
        last_line,
        max_bytes,
        verbatim_fallback_note,
    )
}

/// Shared best-effort reconstruction body: joins `parts` with the
/// kind-appropriate separator, caps at `max_bytes`, and labels the output
/// as a reconstruction (never exact source). The BOUND path feeds it
/// slices of digest-validated retained bytes at authenticated durable
/// spans (r5 C4, verification finding 1); the ordinary unbound path feeds
/// stored chunk text.
#[allow(clippy::too_many_arguments)]
fn stitch_parts(
    path: &str,
    source: &str,
    kind: &str,
    is_code: bool,
    parts: &[&str],
    chunk_count: usize,
    first_line: Option<u32>,
    last_line: Option<u32>,
    max_bytes: usize,
    verbatim_fallback_note: Option<String>,
) -> Result<Value, ToolError> {
    let separator = if is_code { "\n" } else { "\n\n" };
    let full = parts.join(separator);
    let total_bytes = full.len();
    let (content, truncated) = truncate_bytes(&full, max_bytes);

    let mut body = json!({
        "path": path,
        "repository": source,
        "kind": kind,
        "chunk_count": chunk_count,
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
    use engram_domain::CodeGeneration;
    use engram_ingest::git_source::{
        CommittedFile, ResolvedCommit, RetainedSnapshotReader, retain_snapshot,
    };
    use engram_integration::EngramProviderBuilder;
    use engram_integration::code_binding::{
        CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
    };
    use engram_knowledge::CodeBindingRepository;
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
        /// Durable canonical span for the DEFAULT participant
        /// (`chunk-bound-1`) — set by fixtures that vary its claimed span.
        default_span: std::sync::Mutex<Option<engram_domain::CanonicalSpan>>,
        descriptor_digest: std::sync::Mutex<Option<String>>,
        live_lease: std::sync::Mutex<Option<engram_knowledge::CodeReadLease>>,
        /// Deterministic mid-window publication (A4 barrier fixtures): when
        /// `flip_at_descriptor` > 0, the Nth `get_source_descriptor` call
        /// publishes `flip_generation` (retiring the current active) BEFORE
        /// serving the durable descriptor — a REAL publication landing
        /// between participant selection and the byte read, inside the
        /// consistent-read window. No threads, no timing.
        flip_at_descriptor: std::sync::atomic::AtomicUsize,
        descriptor_reads: std::sync::atomic::AtomicUsize,
        flip_applied: std::sync::atomic::AtomicBool,
        flip_generation: std::sync::Mutex<Option<CodeGeneration>>,
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
                default_span: std::sync::Mutex::new(None),
                flip_at_descriptor: std::sync::atomic::AtomicUsize::new(0),
                descriptor_reads: std::sync::atomic::AtomicUsize::new(0),
                flip_applied: std::sync::atomic::AtomicBool::new(false),
                flip_generation: std::sync::Mutex::new(None),
            }
        }

        /// Arms the deterministic mid-window publication: the `at`-th
        /// `get_source_descriptor` call publishes `next` before serving the
        /// (still durable) descriptor of the retiring generation.
        fn arm_descriptor_flip(&self, at: usize, next: CodeGeneration) {
            *self.flip_generation.lock().expect("flip lock") = Some(next);
            self.flip_at_descriptor
                .store(at, std::sync::atomic::Ordering::SeqCst);
        }

        /// Whether the armed publication has actually been applied.
        fn flip_fired(&self) -> bool {
            self.flip_applied.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn flip_armed(&self) -> bool {
            self.flip_at_descriptor
                .load(std::sync::atomic::Ordering::SeqCst)
                > 0
        }

        /// Marks the armed publication applied (idempotent).
        fn apply_flip_if_due(&self) {
            let reads = self
                .descriptor_reads
                .load(std::sync::atomic::Ordering::SeqCst);
            let at = self
                .flip_at_descriptor
                .load(std::sync::atomic::Ordering::SeqCst);
            if at == 0 || reads < at {
                return;
            }
            self.flip_applied
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// The EFFECTIVE binding (post-flip active generation + bumped epoch).
        fn effective_binding(&self) -> engram_domain::CodeBinding {
            if self.flip_applied.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(next) = self.flip_generation.lock().expect("flip lock").clone()
            {
                let mut binding = self.binding.clone();
                binding.active_generation = Some(next.id.clone());
                binding.epoch += 1;
                return binding;
            }
            self.binding.clone()
        }

        /// The EFFECTIVE active generation record.
        fn active_generation(&self) -> CodeGeneration {
            if self.flip_applied.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(next) = self.flip_generation.lock().expect("flip lock").clone()
            {
                return next;
            }
            self.generation.clone()
        }

        /// Records durable lineage for ADDITIONAL participating chunks on
        /// this double's canonical path (`src/bound.py`), each with its
        /// durable canonical line span (final r5 finding 2).
        fn with_chunk_lineage(&self, entries: &[(String, String, u32, u32)]) {
            let mut lineage = self.extra_lineage.lock().expect("lineage lock");
            for (record_id, generation_id, start_line, end_line) in entries {
                lineage.push(engram_knowledge::TargetLineage {
                    record_id: record_id.clone(),
                    repo_id: self.binding.repo_id.0.clone(),
                    generation_id: generation_id.clone(),
                    retired: false,
                    source_path: "src/bound.py".to_owned(),
                    span: Some(engram_domain::CanonicalSpan {
                        start_line: *start_line,
                        end_line: *end_line,
                    }),
                    contributors: Vec::new(),
                });
            }
        }

        /// Records durable lineage for a chunk on an EXPLICIT canonical
        /// path (same-header/different-file fixtures).
        fn with_chunk_lineage_at_path(
            &self,
            entries: &[(String, String, String, Option<(u32, u32)>)],
        ) {
            let mut lineage = self.extra_lineage.lock().expect("lineage lock");
            for (record_id, generation_id, source_path, span) in entries {
                lineage.push(engram_knowledge::TargetLineage {
                    record_id: record_id.clone(),
                    repo_id: self.binding.repo_id.0.clone(),
                    generation_id: generation_id.clone(),
                    retired: false,
                    source_path: source_path.clone(),
                    span: span.map(|(start_line, end_line)| engram_domain::CanonicalSpan {
                        start_line,
                        end_line,
                    }),
                    contributors: Vec::new(),
                });
            }
        }

        /// Records RETIRED durable lineage (a retirement fact on the row's
        /// own generation — positive ownership, denied participation);
        /// path/span are irrelevant (retirement denies at routing).
        fn with_retired_lineage(&self, entries: &[(String, String)]) {
            let mut lineage = self.extra_lineage.lock().expect("lineage lock");
            for (record_id, generation_id) in entries {
                lineage.push(engram_knowledge::TargetLineage {
                    record_id: record_id.clone(),
                    repo_id: self.binding.repo_id.0.clone(),
                    generation_id: generation_id.clone(),
                    retired: true,
                    source_path: "src/bound.py".to_owned(),
                    span: None,
                    contributors: Vec::new(),
                });
            }
        }

        /// Sets the DEFAULT participant's durable canonical span.
        fn set_default_span(&self, start_line: u32, end_line: u32) {
            *self.default_span.lock().expect("default span lock") =
                Some(engram_domain::CanonicalSpan {
                    start_line,
                    end_line,
                });
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
                    .then(|| self.effective_binding()),
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
            let active = self.active_generation();
            if generation_id == active.id {
                return Ok(Some(active));
            }
            // The pre-flip active generation stays durable and readable
            // (its descriptors and snapshot are retained).
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
            // Default: the single-chunk fixture participant on the double's
            // canonical path. Mixed-generation fixtures seed `extra_lineage`
            // for their participating chunks.
            let mut lineage = vec![engram_knowledge::TargetLineage {
                record_id: "chunk-bound-1".to_owned(),
                repo_id: self.binding.repo_id.0.clone(),
                generation_id: self.generation.id.clone(),
                retired: false,
                source_path: "src/bound.py".to_owned(),
                span: *self.default_span.lock().expect("default span lock"),
                contributors: Vec::new(),
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
            self.descriptor_reads
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // The deterministic mid-window publication lands HERE — after
            // the participants were selected and authorized, before the
            // byte read completes the operation.
            self.apply_flip_if_due();
            if scope != &self.binding.scope
                || *repo_id != self.binding.repo_id
                || path != "src/bound.py"
            {
                return Ok(None);
            }
            // Durable descriptors serve their OWN generation (retirement of
            // the active generation does not erase them).
            let generation = if generation_id == self.generation.id {
                self.generation.clone()
            } else if let Some(old) = &self.retained_old
                && old.id == generation_id
            {
                old.clone()
            } else if generation_id == self.active_generation().id {
                self.active_generation()
            } else {
                return Ok(None);
            };
            Ok(Some(engram_domain::IndexedSource {
                repo_id: self.binding.repo_id.clone(),
                generation_id: generation.id.clone(),
                commit_oid: generation.commit_oid.clone(),
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
            let binding = self.effective_binding();
            let published = scope == &self.binding.scope;
            let mut generations = if published {
                vec![self.active_generation().id]
            } else {
                Default::default()
            };
            if let Some(old) = &self.retained_old {
                generations.push(old.id.clone());
            }
            Ok(engram_knowledge::CodeEligibility {
                scope: scope.clone(),
                epoch: if published { binding.epoch } else { 1 },
                generations,
                repository_ids: if published {
                    vec![self.binding.repo_id.0.clone()]
                } else {
                    Default::default()
                },
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
        _scope: &engram_domain::Scope,
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
        // Scanner-realistic chunk text: the FIRST LINE of the claimed span
        // (a real CodeSymbol chunk's text is its declaration line — the
        // same-operation content validation depends on this correspondence).
        let lossy = String::from_utf8_lossy(&source);
        let span_text = lossy
            .lines()
            .nth(start_line.saturating_sub(1) as usize)
            .unwrap_or_else(|| lossy.lines().next().unwrap_or_default());
        let chunk = engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from("chunk-bound-1"),
            document_id: engram_domain::Id::from("doc-bound"),
            source_id: engram_domain::Id::from("src-bound"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: span_text.to_owned(),
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
        let (app, root, _bindings) = bound_app_impl(dir, source.as_bytes().to_vec(), "alpha", 1, 1);
        (app, root)
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
        let (app, root, _bindings) = bound_app_impl(
            dir,
            source.as_bytes().to_vec(),
            symbol,
            start_line,
            end_line,
        );
        (app, root)
    }

    /// `bound_app` over raw (possibly non-UTF-8) bytes.
    fn bound_app_with_symbols_bytes(
        dir: &std::path::Path,
        source: &[u8],
        start_line: u32,
        end_line: u32,
    ) -> (App, std::path::PathBuf) {
        let (app, root, _bindings) =
            bound_app_impl(dir, source.to_vec(), "alpha", start_line, end_line);
        (app, root)
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
    ) -> (App, std::path::PathBuf, std::sync::Arc<InMemoryBindings>) {
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
        // The DEFAULT participant's durable canonical span matches the
        // fixture's staged claim (final r5 finding 2 fixtures validate
        // claims against DURABLE spans, not against stored text).
        bindings.set_default_span(start_line, end_line);
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
            bindings,
        )
    }

    /// [`bound_app_impl`] over EXPLICIT retained files and chunk rows (the
    /// canonical-ID tampering fixtures): the digest is bound to the
    /// `src/bound.py` file bytes, rows are used verbatim, and `lineage`
    /// seeds durable lineage for additional records.
    fn bound_app_with_rows(
        dir: &std::path::Path,
        files: Vec<CommittedFile>,
        rows: Vec<engram_domain::KnowledgeChunk>,
        lineage: &[(String, String, u32, u32)],
    ) -> (App, std::path::PathBuf, std::sync::Arc<InMemoryBindings>) {
        let scope = crate::scope::project_scope("bound-project", "default");
        let commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_owned(),
            oid: "a".repeat(40),
            object_format: "sha1".to_owned(),
        };
        let retained_root = dir.join("retained-rows");
        let snapshot = retain_snapshot(&retained_root, commit, &files).expect("retain snapshot");
        let bindings = std::sync::Arc::new(InMemoryBindings::published(&snapshot.commit.oid));
        let bound_bytes = files
            .iter()
            .find(|file| file.path.as_os_str() == "src/bound.py")
            .map(|file| file.bytes.clone())
            .expect("fixture carries src/bound.py");
        bindings.set_descriptor_digest(
            engram_ingest::content_hash(&bound_bytes)
                .trim_start_matches("sha256:")
                .to_string(),
        );
        bindings.with_chunk_lineage(lineage);
        let knowledge: std::sync::Arc<dyn engram_integration::KnowledgeQuery> =
            std::sync::Arc::new(SeededQuery { chunks: rows });
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
            bindings,
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
            ("chunk-alpha-1".to_owned(), "gen-1".to_owned(), 1, 2),
            ("chunk-alpha-2".to_owned(), "gen-1".to_owned(), 4, 5),
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
        _scope: &engram_domain::Scope,
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
        // A4 (r5): a DIFFERENT-SOURCE impostor beside the legitimate current
        // participants — unstamped, no durable lineage, name `alpha` at
        // wrong lines. It can neither participate nor force an AMBIGUITY
        // response for this bound path: ambiguity is decided AFTER canonical
        // participant filtering, and the bound answer wins.
        let mut impostor_foreign = chunk_of(
            "chunk-impostor-foreign-src",
            "gen-2",
            "alpha",
            42,
            42,
            "def impostor():",
        );
        impostor_foreign.provenance.source = "impostor-foreign-source".to_owned();
        impostor_foreign.metadata = None;
        chunks.push(impostor_foreign);
        // Canonical durable lineage: the OLD chunks belong to the retired
        // gen-1 (not participants) and the CURRENT chunks to gen-2; the
        // unstamped and forged-stamp IMPOSTORS deliberately carry NO
        // lineage — stamps are not admissible evidence.
        bindings.with_chunk_lineage(&[
            ("chunk-old-alpha".to_owned(), "gen-1".to_owned(), 1, 2),
            ("chunk-old-gamma".to_owned(), "gen-1".to_owned(), 4, 5),
            ("chunk-new-beta".to_owned(), "gen-2".to_owned(), 2, 3),
            ("chunk-new-alpha".to_owned(), "gen-2".to_owned(), 6, 7),
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

    /// Builds the OLD-ONLY fixture: the path's ONLY rows carry durable
    /// gen-1 lineage (the retained superseded generation) while gen-2 is the
    /// published active generation — with or without forged stamps. No
    /// current participant exists for this path.
    fn old_only_app(dir: &std::path::Path, forged_stamps: bool) -> App {
        let scope = crate::scope::project_scope("bound-project", "default");
        let old_source = b"def oldent():\r\n    return 9\r\n".to_vec();
        let current_source = b"import os\r\ndef current():\r\n    return 2\r\n".to_vec();
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
        let retained_root = dir.join("retained-oldonly");
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
        let bindings = std::sync::Arc::new(InMemoryBindings::published_two_generations(
            &old_commit.oid,
            &current_commit.oid,
        ));
        bindings.set_descriptor_digest(
            engram_ingest::content_hash(&current_source)
                .trim_start_matches("sha256:")
                .to_string(),
        );
        let mut metadata = engram_domain::Metadata::new();
        if forged_stamps {
            metadata.insert(
                "codeGenerationId".to_owned(),
                serde_json::Value::String("gen-2".to_owned()),
            );
            metadata.insert(
                "codeRepoId".to_owned(),
                serde_json::Value::String("bound-repo".to_owned()),
            );
        }
        let row = engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from("chunk-oldonly-1"),
            document_id: engram_domain::Id::from("doc-oldonly"),
            source_id: engram_domain::Id::from("src-oldonly"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: "def oldent():".to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: Some(1),
                end_line: Some(2),
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
            entities: vec![engram_domain::EntityRef {
                id: Some(engram_domain::Id::from("chunk-oldonly-1")),
                kind: None,
                name: Some("oldent".to_owned()),
                aliases: Vec::new(),
            }],
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: "sha256:chunk-oldonly-1".to_owned(),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: if forged_stamps { Some(metadata) } else { None },
        };
        // Durable lineage ONLY on the retained old generation — the row is
        // provably BOUND CODE (this is not an unbound path).
        bindings.with_chunk_lineage(&[("chunk-oldonly-1".to_owned(), "gen-1".to_owned(), 1, 2)]);
        let knowledge: std::sync::Arc<dyn engram_integration::KnowledgeQuery> =
            std::sync::Arc::new(SeededQuery { chunks: vec![row] });
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
        App {
            provider,
            scope,
            ontology: OntologyConfig::default(),
            taxonomy: TaxonomyConfig::default(),
            storage_dir: dir.to_path_buf(),
        }
    }

    /// A4 (r5, the resurrection regression): UNSTAMPED rows whose durable
    /// lineage points ONLY at a retained old generation DENY in EVERY mode —
    // the classification is lineage-based (never stamp-based), so there is
    /// no verbatim and no chunk-stitching fallback for bound code.
    #[test]
    fn old_only_lineage_without_stamps_denies_all_modes() {
        let dir = tempfile::tempdir().unwrap();
        let app = old_only_app(dir.path(), false);
        for args in [
            json!({ "path": "src/bound.py" }),
            json!({ "path": "src/bound.py", "mode": "chunks" }),
            json!({ "path": "src/bound.py", "mode": "verbatim" }),
            json!({ "path": "src/bound.py", "symbol": "oldent" }),
        ] {
            let err = fetch_content(&app, &args).unwrap_err();
            assert!(
                err.message.contains("no current canonical participants"),
                "unstamped old-only lineage must deny without fallback ({args}): {}",
                err.message
            );
        }
    }

    /// Final r5 C3 (finding 1): a RETIREMENT-FACT fixture. The row is
    /// UNSTAMPED and its durable lineage sits on the CURRENT published
    /// generation with `retired: true` — `code_lineage_of()` resolves it to
    /// `None`, which is exactly the shape the old router skipped without
    /// recording noncurrent ownership. `with_current` adds a legitimate
    /// current participant (alpha, lines 1–2) beside the retired row
    /// (retiredfn, lines 4–5).
    fn retired_fact_app(dir: &std::path::Path, with_current: bool) -> App {
        let source =
            b"def alpha():\r\n    return 1\r\n\r\ndef retiredfn():\r\n    return 9\r\n".to_vec();
        let row = |id: &str, name: &str, start: u32, end: u32| {
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from(id),
                document_id: engram_domain::Id::from("doc-retired"),
                source_id: engram_domain::Id::from("src-retired"),
                kind: KnowledgeChunkKind::CodeSymbol,
                text: format!("def {name}():"),
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
                    name: Some(name.to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: format!("sha256:{id}"),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                // UNSTAMPED: no metadata at all — the durable lineage (and
                // its retirement fact) is the only bound-code evidence.
                metadata: None,
            }
        };
        let retired_row = row("chunk-retired-1", "retiredfn", 4, 5);
        let current_row = row("chunk-current-1", "alpha", 1, 2);
        let rows = if with_current {
            vec![current_row, retired_row]
        } else {
            vec![retired_row]
        };
        let current_lineage: &[(String, String, u32, u32)] = if with_current {
            &[("chunk-current-1".to_owned(), "gen-1".to_owned(), 1, 2)]
        } else {
            &[]
        };
        let (app, _root, bindings) = bound_app_with_rows(
            dir,
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: source,
            }],
            rows,
            current_lineage,
        );
        // The retirement fact on the CURRENT generation — the branch the
        // old-generation denial can never reach (this generation IS
        // published).
        bindings.with_retired_lineage(&[("chunk-retired-1".to_owned(), "gen-1".to_owned())]);
        app
    }

    /// Final r5 C3 (finding 1, the decisive retired-fact regression —
    /// DISTINCT from old-generation lineage): an UNSTAMPED chunk with a
    /// retirement fact on the CURRENT generation DENIES in EVERY mode,
    /// including the fallback consumption paths. The stamp backstop cannot
    /// catch it (there are no stamps); the lineage/retirement facts must.
    #[test]
    fn retired_fact_unstamped_chunk_denies_every_mode() {
        let dir = tempfile::tempdir().unwrap();
        let app = retired_fact_app(dir.path(), false);
        for args in [
            json!({ "path": "src/bound.py" }),
            json!({ "path": "src/bound.py", "mode": "chunks" }),
            json!({ "path": "src/bound.py", "mode": "verbatim" }),
            json!({ "path": "src/bound.py", "symbol": "retiredfn" }),
        ] {
            let err = fetch_content(&app, &args).unwrap_err();
            assert!(
                err.message.contains("no current canonical participants"),
                "an unstamped retired-fact row must DENY in every mode ({args}): {}",
                err.message
            );
        }
    }

    /// Final r5 C3 (finding 1): a retired unstamped row BESIDE a current
    /// participant — the current participant serves (exact path read, chunks
    /// fallback, and its own symbol span) while the retired row NEVER leaks:
    /// the chunks fallback consumes only validated current participants, and
    /// the retired symbol has no selectable span.
    #[test]
    fn retired_row_beside_current_participant_serves_current_only() {
        let dir = tempfile::tempdir().unwrap();
        let app = retired_fact_app(dir.path(), true);

        // The exact path read serves the digest-validated whole blob.
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
        assert_eq!(out["source"], "exact");

        // The chunks fallback stitches ONLY the validated current
        // participant — the retired row's stored text must not appear.
        let chunks = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
        );
        let chunks_text = chunks["content"].as_str().unwrap();
        assert!(
            chunks_text.contains("def alpha():"),
            "the current participant's text serves: {chunks_text}"
        );
        assert!(
            !chunks_text.contains("def retiredfn():"),
            "the retired row never leaks into the chunks fallback: {chunks_text}"
        );

        // The current participant's symbol span serves byte-exactly.
        let symbol = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" })).unwrap(),
        );
        assert_eq!(symbol["source"], "exact");
        assert_eq!(
            symbol["content"].as_str().unwrap(),
            "def alpha():\r\n    return 1\r\n",
            "the current participant's symbol span is byte-exact"
        );

        // The retired symbol has no selectable span in the indexed
        // generation — a request naming it denies (no resurrection).
        let retired_symbol = fetch_content(
            &app,
            &json!({ "path": "src/bound.py", "symbol": "retiredfn" }),
        )
        .unwrap_err();
        assert!(
            retired_symbol.message.contains("symbol selection failed"),
            "the retired symbol is not selectable: {}",
            retired_symbol.message
        );
    }

    /// Final r5 C4 (verification finding 1, the DECISIVE payload-substitution
    /// regression): a current participant with UNCHANGED id, canonical path
    /// and EXACT canonical span whose STORED TEXT was substituted — first
    /// with the retired neighbor's text, then with a different file's
    /// content — rides through NOTHING: the bound chunks reconstruction
    /// emits only authenticated retained bytes (each participant's durable
    /// span sliced out of the digest-validated revision blob), so the
    /// substituted text is never served; a legitimate untouched participant
    /// still serves correctly beside it, and even the substituted row's own
    /// symbol serves its AUTHENTICATED span bytes.
    fn substituted_text_app(dir: &std::path::Path, beta_text: &str) -> App {
        let source = b"def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n\r\ndef retiredfn():\r\n    return 9\r\n".to_vec();
        let row = |id: &str, name: &str, start: u32, end: u32, text: &str| {
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from(id),
                document_id: engram_domain::Id::from("doc-substituted"),
                source_id: engram_domain::Id::from("src-substituted"),
                kind: KnowledgeChunkKind::CodeSymbol,
                text: text.to_owned(),
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
                    name: Some(name.to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: format!("sha256:{id}"),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }
        };
        // The UNTOUCHED legitimate participant (alpha, lines 1–2), the
        // SUBSTITUTED current participant (beta, lines 4–5 — id/path/span
        // exactly as published, only `text` replaced), and the retired
        // neighbor (retiredfn, lines 7–8) supplying the substitution
        // payload for the first variant.
        let rows = vec![
            row("chunk-current-1", "alpha", 1, 2, "def alpha():"),
            row("chunk-current-2", "beta", 4, 5, beta_text),
            row("chunk-retired-1", "retiredfn", 7, 8, "def retiredfn():"),
        ];
        let (app, _root, bindings) = bound_app_with_rows(
            dir,
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: source,
            }],
            rows,
            &[
                ("chunk-current-1".to_owned(), "gen-1".to_owned(), 1, 2),
                ("chunk-current-2".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        bindings.with_retired_lineage(&[("chunk-retired-1".to_owned(), "gen-1".to_owned())]);
        app
    }

    #[test]
    fn substituted_current_text_cannot_ride_chunks_mode() {
        for (label, beta_text, forbidden) in [
            (
                "retired neighbor's text",
                "def retiredfn():",
                "def retiredfn():",
            ),
            (
                "another file's content",
                "FOREIGN PAYLOAD secret-token from a different file\nimpersonating beta",
                "FOREIGN PAYLOAD secret-token",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let app = substituted_text_app(dir.path(), beta_text);

            // chunks mode serves ONLY authenticated bytes: alpha's and
            // beta's durable spans sliced from the retained revision, in
            // durable-span order — never the substituted stored text.
            let chunks = content_of(
                &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
            );
            let chunks_text = chunks["content"].as_str().unwrap();
            assert_eq!(chunks["source"], "chunks");
            assert_eq!(chunks["chunk_count"], 2, "both current participants count");
            assert_eq!(chunks["lines"]["first"], 1);
            assert_eq!(chunks["lines"]["last"], 5);
            let alpha_at = chunks_text
                .find("def alpha():\r\n    return 1")
                .unwrap_or_else(|| panic!("alpha's authenticated span serves: {chunks_text}"));
            let beta_at = chunks_text
                .find("def beta():\r\n    return 2")
                .unwrap_or_else(|| panic!("beta's authenticated span serves: {chunks_text}"));
            assert!(
                alpha_at < beta_at,
                "participants stitch in durable-span order: {chunks_text}"
            );
            assert!(
                !chunks_text.contains(forbidden),
                "the substituted stored text ({label}) is NEVER served: {chunks_text}"
            );
            assert!(
                !chunks_text.contains("def retiredfn():"),
                "the retired neighbor's lines are in no authenticated span: {chunks_text}"
            );

            // The exact path read is untouched by the substitution: the
            // whole digest-validated revision still serves.
            let exact =
                content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
            assert_eq!(exact["source"], "exact");
            assert!(
                exact["content"]
                    .as_str()
                    .unwrap()
                    .contains("def retiredfn():"),
                "the exact read serves the true revision (retiredfn's lines \
                 ARE retained bytes): {}",
                exact["content"]
            );

            // The legitimate untouched participant still serves correctly
            // beside the substituted row — byte-exact symbol span.
            let alpha = content_of(
                &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" }))
                    .unwrap(),
            );
            assert_eq!(alpha["source"], "exact");
            assert_eq!(
                alpha["content"].as_str().unwrap(),
                "def alpha():\r\n    return 1\r\n",
                "the untouched participant serves byte-exactly"
            );

            // Even the SUBSTITUTED row's own symbol serves its
            // AUTHENTICATED span bytes, never its stored text.
            let beta = content_of(
                &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "beta" })).unwrap(),
            );
            assert_eq!(beta["source"], "exact");
            assert_eq!(
                beta["content"].as_str().unwrap(),
                "def beta():\r\n    return 2\r\n",
                "the substituted row's symbol serves authenticated bytes only"
            );
        }
    }

    /// Fixture provenance with an EXPLICIT source label (r5 C5
    /// label-provenance and reconstruction-byte regressions).
    fn label_provenance(source: &str) -> engram_domain::Provenance {
        let mut provenance = fixture_provenance();
        provenance.source = source.to_owned();
        provenance
    }

    fn labeled_row(
        id: &str,
        name: &str,
        start: u32,
        end: u32,
        text: &str,
        source: &str,
    ) -> engram_domain::KnowledgeChunk {
        engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from(id),
            document_id: engram_domain::Id::from("doc-c5"),
            source_id: engram_domain::Id::from("src-c5"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: text.to_owned(),
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
                name: Some(name.to_owned()),
                aliases: Vec::new(),
            }],
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: format!("sha256:{id}"),
            provenance: label_provenance(source),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }
    }

    /// r5 C5 (verification defect 1a): a retained participant span that is
    /// NOT valid UTF-8 makes chunks mode DENY with an explicit pointer to
    /// the labeled exact read — the old stored-text stitching could serve
    /// text that cannot have been the revision's bytes. On the SAME fixture
    /// the default exact read keeps its established behavior (labeled hex
    /// of the exact bytes).
    #[test]
    fn invalid_utf8_span_denies_chunks_mode_with_labeled_exact_read_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let source_bytes: Vec<u8> = b"def alpha():\r\n    x = '"
            .to_vec()
            .into_iter()
            .chain([0x80, 0x81, 0x82])
            .chain(b"'\r\n".to_vec())
            .collect();
        let (app, _retained) = bound_app_with_symbols_bytes(dir.path(), &source_bytes, 1, 2);

        // chunks mode DENIES on the invalid-UTF-8 retained span, with the
        // participant identified and the labeled exact read named as the
        // alternative.
        let err =
            fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap_err();
        assert!(
            err.message
                .contains("bound chunks reconstruction unavailable")
                && err.message.contains("'chunk-bound-1'")
                && err
                    .message
                    .contains("not valid UTF-8 in the retained revision")
                && err.message.contains("use the default exact read"),
            "chunks mode must deny with the explicit exact-read pointer: {}",
            err.message
        );

        // The established labeled exact read continues on the SAME fixture.
        let exact = content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
        assert_eq!(exact["source"], "exact");
        assert_eq!(exact["encoding"], "hex");
        assert_eq!(
            exact["content"].as_str().unwrap().len(),
            source_bytes.len() * 2,
            "the exact read still hex-encodes every exact byte"
        );
    }

    /// r5 C5 (verification defect 1b): the chunks-mode repository label
    /// comes from a PARTICIPANT row — a differently labeled NONPARTICIPANT
    /// row that merely matched the path and sorts FIRST can never supply
    /// the label for content it did not produce.
    #[test]
    fn repository_label_comes_from_a_participant_not_the_first_matched_row() {
        let dir = tempfile::tempdir().unwrap();
        let source = b"def alpha():\n    return 1\n\ndef beta():\n    return 2\n".to_vec();
        // The decoy claims the EARLIEST span (sorts first in `ordered`) and
        // carries a DIFFERENT provenance label; it has no durable lineage, so
        // it is a nonparticipant. Both participants carry their own labels.
        let rows = vec![
            labeled_row(
                "chunk-label-decoy",
                "decoy",
                1,
                1,
                "def alpha():",
                "impostor-repository-label",
            ),
            labeled_row(
                "chunk-label-alpha",
                "alpha",
                1,
                2,
                "def alpha():",
                "participant-alpha-source",
            ),
            labeled_row(
                "chunk-label-beta",
                "beta",
                4,
                5,
                "def beta():",
                "participant-beta-source",
            ),
        ];
        let (app, _root, _bindings) = bound_app_with_rows(
            dir.path(),
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: source,
            }],
            rows,
            &[
                ("chunk-label-alpha".to_owned(), "gen-1".to_owned(), 1, 2),
                ("chunk-label-beta".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
        );
        assert_eq!(
            out["repository"], "participant-alpha-source",
            "the label comes from the FIRST PARTICIPANT row, never the earlier nonparticipant"
        );
        assert_ne!(
            out["repository"], "impostor-repository-label",
            "a nonparticipant row can never label content it did not produce"
        );
        assert_eq!(
            out["chunk_count"], 2,
            "only the two participants count; the decoy is not a participant"
        );
    }

    /// r5 C5 (verification defect 1c): the bound chunks reconstruction is
    /// byte-faithful to the RETAINED SPAN BYTES, not to stored text — each
    /// participant's terminal newline is PRESERVED and the stitch
    /// separator lands AFTER the newline-terminated part (LF fixture), and
    /// CRLF spans pass through VERBATIM (CRLF fixture). `total_bytes` counts
    /// the reconstructed byte stream (terminal newlines + separators), not
    /// stored-text length.
    #[test]
    fn bound_chunks_reconstruction_preserves_terminal_newlines_and_crlf() {
        let cases = [
            (
                "LF: terminal newlines preserved, separator after the newline-terminated part",
                b"def alpha():\n    return 1\n\ndef beta():\n    return 2\n".to_vec(),
                "def alpha():\n    return 1\n".to_owned() + "\n" + "def beta():\n    return 2\n",
            ),
            (
                "CRLF: span bytes preserved verbatim",
                b"def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n".to_vec(),
                "def alpha():\r\n    return 1\r\n".to_owned()
                    + "\n"
                    + "def beta():\r\n    return 2\r\n",
            ),
        ];
        for (label, source, expected) in cases {
            let dir = tempfile::tempdir().unwrap();
            let rows = vec![
                labeled_row(
                    "chunk-bytes-alpha",
                    "alpha",
                    1,
                    2,
                    "def alpha():",
                    "participant-alpha-source",
                ),
                labeled_row(
                    "chunk-bytes-beta",
                    "beta",
                    4,
                    5,
                    "def beta():",
                    "participant-beta-source",
                ),
            ];
            let (app, _root, _bindings) = bound_app_with_rows(
                dir.path(),
                vec![CommittedFile {
                    path: "src/bound.py".into(),
                    oid: "b".repeat(40),
                    mode: "100644".to_owned(),
                    bytes: source,
                }],
                rows,
                &[
                    ("chunk-bytes-alpha".to_owned(), "gen-1".to_owned(), 1, 2),
                    ("chunk-bytes-beta".to_owned(), "gen-1".to_owned(), 4, 5),
                ],
            );
            let out = content_of(
                &fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap(),
            );
            let content = out["content"].as_str().unwrap();
            assert_eq!(content, expected, "reconstructed bytes ({label})");
            assert_eq!(
                out["total_bytes"],
                json!(expected.len()),
                "total_bytes counts the reconstructed byte stream ({label})"
            );
            assert_eq!(out["truncated"], false, "no cap requested ({label})");
            assert_eq!(out["chunk_count"], 2);
        }
    }

    /// r5 C5 (verification defect 1c): the `max_bytes` cap boundary applies
    /// to the NEW byte stream (retained terminal newlines + stitch
    /// separators) — `total_bytes` is the full reconstructed length and the
    /// truncation boundary cuts inside it, which stored-text stitching
    /// could never produce (24 stored bytes vs 52 reconstructed here).
    #[test]
    fn bound_chunks_cap_applies_to_reconstructed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = b"def alpha():\n    return 1\n\ndef beta():\n    return 2\n".to_vec();
        let rows = vec![
            labeled_row(
                "chunk-cap-alpha",
                "alpha",
                1,
                2,
                "def alpha():",
                "participant-alpha-source",
            ),
            labeled_row(
                "chunk-cap-beta",
                "beta",
                4,
                5,
                "def beta():",
                "participant-beta-source",
            ),
        ];
        let (app, _root, _bindings) = bound_app_with_rows(
            dir.path(),
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: source,
            }],
            rows,
            &[
                ("chunk-cap-alpha".to_owned(), "gen-1".to_owned(), 1, 2),
                ("chunk-cap-beta".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        let expected =
            "def alpha():\n    return 1\n".to_owned() + "\n" + "def beta():\n    return 2\n";

        // BELOW the boundary: the cap cuts inside the reconstructed stream
        // (30 bytes is already past the 24-byte stored-text stitch — only
        // the retained-terminal-newline stream can supply it).
        let cut = content_of(
            &fetch_content(
                &app,
                &json!({ "path": "src/bound.py", "mode": "chunks", "max_bytes": 30 }),
            )
            .unwrap(),
        );
        assert_eq!(cut["total_bytes"], json!(expected.len()));
        assert_eq!(cut["truncated"], true);
        assert_eq!(cut["returned_bytes"], 30);
        assert_eq!(cut["max_bytes"], 30);
        assert_eq!(
            cut["content"].as_str().unwrap(),
            &expected[..30],
            "the cap cuts the RECONSTRUCTED byte stream at 30 bytes"
        );

        // AT the boundary: exactly the reconstructed length serves whole.
        let whole = content_of(
            &fetch_content(
                &app,
                &json!({ "path": "src/bound.py", "mode": "chunks", "max_bytes": expected.len() }),
            )
            .unwrap(),
        );
        assert_eq!(whole["total_bytes"], json!(expected.len()));
        assert_eq!(whole["truncated"], false);
        assert_eq!(whole["returned_bytes"], json!(expected.len()));
        assert_eq!(whole["content"].as_str().unwrap(), expected);
    }

    /// Final r5 C3 (finding 2, the decisive widened-span regression): a
    /// WIDENED legitimate span — alpha's true lines 1–2 claimed as 1–5 so
    /// the span would encompass beta — can never smuggle the adjacent
    /// symbol: the claim is validated against the DURABLE canonical span
    /// (1–2) and denies; a first-line match inside an enlarged span
    /// authenticates nothing (the C2-era substring check is gone).
    #[test]
    fn widened_span_cannot_smuggle_adjacent_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let bound = b"def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n".to_vec();
        let row = |id: &str, name: &str, start: u32, end: u32, text: &str| {
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from(id),
                document_id: engram_domain::Id::from("doc-widen"),
                source_id: engram_domain::Id::from("src-widen"),
                kind: KnowledgeChunkKind::CodeSymbol,
                text: text.to_owned(),
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
                    name: Some(name.to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: format!("sha256:{id}"),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: None,
            }
        };
        // alpha's row WIDENS its legitimate 1–2 to 1–5 (beta's first line
        // still matches inside the enlarged span — the old predicate's
        // flaw); beta's row is entirely legitimate.
        let rows = vec![
            row("chunk-widen-alpha", "alpha", 1, 5, "def alpha():"),
            row("chunk-widen-beta", "beta", 4, 5, "def beta():"),
        ];
        let (app, _root, _bindings) = bound_app_with_rows(
            dir.path(),
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: bound,
            }],
            rows,
            &[
                // DURABLE canonical spans: alpha 1–2, beta 4–5.
                ("chunk-widen-alpha".to_owned(), "gen-1".to_owned(), 1, 2),
                ("chunk-widen-beta".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        for args in [
            json!({ "path": "src/bound.py" }),
            json!({ "path": "src/bound.py", "symbol": "alpha" }),
        ] {
            let err = fetch_content(&app, &args).unwrap_err();
            assert!(
                err.message
                    .contains("span that does not match the retained revision")
                    && err.message.contains("canonical lines 1-2"),
                "the widened 1-5 claim denies against the durable 1-2 span ({args}): {}",
                err.message
            );
        }
    }

    /// Final r5 C3 (finding 2, the decisive same-header/different-file
    /// regression): a current-lineage row whose durable canonical source
    /// path is ANOTHER FILE denies even though its declaration header,
    /// claimed span, repository and generation all match the request —
    /// routing repository/generation never authenticates path identity.
    #[test]
    fn same_header_different_file_denies() {
        let dir = tempfile::tempdir().unwrap();
        // BOTH files carry the SAME declaration header at the SAME lines —
        // content evidence cannot distinguish them; only the durable path
        // association can.
        let bound = b"def alpha():\r\n    return 1\r\n".to_vec();
        let other = b"def alpha():\r\n    return 7\r\n".to_vec();
        let row = |id: &str, name: &str| engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from(id),
            document_id: engram_domain::Id::from("doc-samehdr"),
            source_id: engram_domain::Id::from("src-samehdr"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: "def alpha():".to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: Some(1),
                end_line: Some(2),
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
            entities: vec![engram_domain::EntityRef {
                id: Some(engram_domain::Id::from(id)),
                kind: None,
                name: Some(name.to_owned()),
                aliases: Vec::new(),
            }],
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: format!("sha256:{id}"),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        };
        let (app, _root, bindings) = bound_app_with_rows(
            dir.path(),
            vec![
                CommittedFile {
                    path: "src/bound.py".into(),
                    oid: "b".repeat(40),
                    mode: "100644".to_owned(),
                    bytes: bound,
                },
                CommittedFile {
                    path: "src/other.py".into(),
                    oid: "c".repeat(40),
                    mode: "100644".to_owned(),
                    bytes: other,
                },
            ],
            vec![
                row("chunk-samehdr-real", "alpha"),
                row("chunk-samehdr-moved", "alpha"),
            ],
            &[("chunk-samehdr-real".to_owned(), "gen-1".to_owned(), 1, 2)],
        );
        // The moved row's durable canonical association is the OTHER file
        // (same header, same generation, same repository).
        bindings.with_chunk_lineage_at_path(&[(
            "chunk-samehdr-moved".to_owned(),
            "gen-1".to_owned(),
            "src/other.py".to_owned(),
            Some((1, 2)),
        )]);
        for args in [
            json!({ "path": "src/bound.py" }),
            json!({ "path": "src/bound.py", "symbol": "alpha" }),
        ] {
            let err = fetch_content(&app, &args).unwrap_err();
            assert!(
                err.message.contains("canonical source path differs"),
                "a same-header row from another file denies ({args}): {}",
                err.message
            );
        }
    }

    /// Final r5 C3 (finding 2, repeated headers): when the same declaration
    /// header occurs at multiple places in one file, a symbol request
    /// serves ONLY the record's durable canonical occurrence — never an
    /// enlarged span covering both, and never a silent merge (two records
    /// with distinct canonical spans stay AMBIGUOUS).
    #[test]
    fn repeated_header_selects_only_canonical_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let bound =
            b"def alpha():\r\n    return 1\r\n\r\ndef alpha():\r\n    return 2\r\n".to_vec();
        let row = |id: &str, start: u32, end: u32| engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from(id),
            document_id: engram_domain::Id::from("doc-repeated"),
            source_id: engram_domain::Id::from("src-repeated"),
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
            metadata: None,
        };
        let file = || {
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: bound.clone(),
            }]
        };

        // ONE record whose durable canonical occurrence is the SECOND
        // header (lines 4–5): the symbol serves exactly those bytes.
        let (app, _root, _bindings) = bound_app_with_rows(
            &dir.path().join("one"),
            file(),
            vec![row("chunk-repeated-2", 4, 5)],
            &[("chunk-repeated-2".to_owned(), "gen-1".to_owned(), 4, 5)],
        );
        let out = content_of(
            &fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" })).unwrap(),
        );
        assert_eq!(out["source"], "exact");
        assert_eq!(
            out["content"].as_str().unwrap(),
            "def alpha():\r\n    return 2\r\n",
            "only the CANONICAL occurrence (lines 4-5) serves — never an enlarged \
             span covering both occurrences"
        );
        let second_offset = bound
            .windows(4)
            .enumerate()
            .filter(|(_, window)| window == b"def ")
            .nth(1)
            .expect("two occurrences")
            .0;
        assert_eq!(
            out["span"]["start"],
            json!(second_offset),
            "the served span starts at the second occurrence's byte offset"
        );

        // TWO records with DISTINCT canonical occurrences: the symbol stays
        // AMBIGUOUS — the occurrences are never silently merged into one
        // enlarged span.
        let (app2, _root2, _bindings2) = bound_app_with_rows(
            &dir.path().join("two"),
            file(),
            vec![row("chunk-repeated-a", 1, 2), row("chunk-repeated-b", 4, 5)],
            &[
                ("chunk-repeated-a".to_owned(), "gen-1".to_owned(), 1, 2),
                ("chunk-repeated-b".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        let err = fetch_content(&app2, &json!({ "path": "src/bound.py", "symbol": "alpha" }))
            .unwrap_err();
        assert!(
            err.message.contains("symbol selection failed") && err.message.contains("ambiguous"),
            "two distinct canonical occurrences stay ambiguous: {}",
            err.message
        );
    }

    /// Final r5 C3 (finding 2, malformed claims REJECT — never normalize,
    /// never clamp, never skip): a participant without a claimed span, one
    /// claiming line 0, reversed bounds, and an end beyond the revision all
    /// deny with explicit malformed-bounds reasons; a missing span does not
    /// skip validation.
    #[test]
    fn malformed_and_missing_span_claims_reject() {
        let dir = tempfile::tempdir().unwrap();
        let bound = b"def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n".to_vec();
        let row = |id: &str, start: Option<u32>, end: Option<u32>| engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from(id),
            document_id: engram_domain::Id::from("doc-malformed"),
            source_id: engram_domain::Id::from("src-malformed"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: "def alpha():".to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: start,
                end_line: end,
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
            metadata: None,
        };
        let file = || {
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: bound.clone(),
            }]
        };
        // (claim, expected denial fragment) — the DURABLE canonical span is
        // the legitimate (1, 2) in every case.
        let cases: [(Option<u32>, Option<u32>, &str); 4] = [
            (None, None, "claims no line span"),
            (Some(0), Some(2), "start line 0"),
            (Some(3), Some(2), "reversed bounds reject"),
            (Some(1), Some(99), "never clamped"),
        ];
        for (index, (start, end, fragment)) in cases.into_iter().enumerate() {
            let (app, _root, _bindings) = bound_app_with_rows(
                &dir.path().join(format!("case-{index}")),
                file(),
                vec![row(&format!("chunk-malformed-{index}"), start, end)],
                &[(format!("chunk-malformed-{index}"), "gen-1".to_owned(), 1, 2)],
            );
            let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
            assert!(
                err.message.contains(fragment),
                "claim ({start:?}, {end:?}) must reject with `{fragment}`: {}",
                err.message
            );
        }
    }

    /// A4 (r5): an IMPOSTOR-ONLY path — rows carrying FORGED current stamps
    /// but no durable lineage — denies too (stamps are an impostor's claim,
    /// not authority).
    #[test]
    fn impostor_only_forged_stamps_deny() {
        let dir = tempfile::tempdir().unwrap();
        let app = old_only_app(dir.path(), true);
        let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            err.message.contains("no current canonical participants"),
            "forged-stamp-only rows must deny: {}",
            err.message
        );
        let chunks_err =
            fetch_content(&app, &json!({ "path": "src/bound.py", "mode": "chunks" })).unwrap_err();
        assert!(
            chunks_err
                .message
                .contains("no current canonical participants"),
            "forged stamps never resurrect through chunks mode: {}",
            chunks_err.message
        );
    }

    /// A4 (r5): a DIFFERENT-SOURCE impostor beside a legitimate current
    /// participant cannot force an AMBIGUITY response — ambiguity is decided
    /// AFTER canonical participant filtering and the bound answer wins.
    #[test]
    fn different_source_impostor_beside_participant_is_not_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _retained, current_oid, _len) = mixed_generation_app(dir.path());
        let out = content_of(&fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap());
        assert_eq!(
            out["source"], "exact",
            "the bound answer wins over the foreign-source impostor: {out}"
        );
        assert_eq!(
            out["status"],
            serde_json::Value::Null,
            "no ambiguity status"
        );
        assert_eq!(out["revision"], current_oid);
        // The impostor's text is nowhere in the served bytes.
        let content = out["content"].as_str().unwrap();
        assert!(
            !content.contains("impostor"),
            "the different-source impostor never contributes bytes: {content}"
        );
    }

    /// A4 (r5): a CANONICAL-ID row (current durable lineage) with a CHANGED
    /// SPAN — its stored declaration line no longer appears inside its
    /// claimed span of the digest-validated revision — denies; current
    /// durable-ID membership alone never authenticates a mutable row's span.
    #[test]
    fn canonical_id_changed_span_denies() {
        let dir = tempfile::tempdir().unwrap();
        let bound = b"def alpha():\r\n    return 1\r\n\r\ndef beta():\r\n    return 2\r\n".to_vec();
        let row = |id: &str, name: &str, start: u32, end: u32, text: &str, stamps: bool| {
            let mut metadata = engram_domain::Metadata::new();
            if stamps {
                metadata.insert(
                    "codeGenerationId".to_owned(),
                    serde_json::Value::String("gen-1".to_owned()),
                );
                metadata.insert(
                    "codeRepoId".to_owned(),
                    serde_json::Value::String("bound-repo".to_owned()),
                );
            }
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from(id),
                document_id: engram_domain::Id::from("doc-span"),
                source_id: engram_domain::Id::from("src-span"),
                kind: KnowledgeChunkKind::CodeSymbol,
                text: text.to_owned(),
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
                    name: Some(name.to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: format!("sha256:{id}"),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: if stamps { Some(metadata) } else { None },
            }
        };
        // Legit participant: alpha at its true lines (1-2). Tampered
        // participant: beta's declaration text with a CHANGED span claiming
        // lines (1-1) — beta's text does not appear in line 1 of the blob.
        let rows = vec![
            row("chunk-span-real", "alpha", 1, 2, "def alpha():", true),
            row("chunk-span-tampered", "beta", 1, 1, "def beta():", true),
        ];
        let (app, _root, _bindings) = bound_app_with_rows(
            dir.path(),
            vec![CommittedFile {
                path: "src/bound.py".into(),
                oid: "b".repeat(40),
                mode: "100644".to_owned(),
                bytes: bound,
            }],
            rows,
            &[
                ("chunk-span-real".to_owned(), "gen-1".to_owned(), 1, 2),
                // The tampered row's DURABLE canonical span is beta's true
                // span (4-5); its mutable claim says 1-1.
                ("chunk-span-tampered".to_owned(), "gen-1".to_owned(), 4, 5),
            ],
        );
        let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            err.message
                .contains("span that does not match the retained revision"),
            "a changed span on a canonical-ID row denies: {}",
            err.message
        );
        let symbol_err =
            fetch_content(&app, &json!({ "path": "src/bound.py", "symbol": "alpha" })).unwrap_err();
        assert!(
            symbol_err
                .message
                .contains("span that does not match the retained revision"),
            "the tampered participant denies the whole path, symbol or not: {}",
            symbol_err.message
        );
    }

    /// A4 (r5): a CANONICAL-ID row for a DIFFERENT FILE whose location.path
    /// was CHANGED to this path — its true content never appears at its
    /// claimed span of this file's digest-validated blob — denies.
    #[test]
    fn canonical_id_changed_path_denies() {
        let dir = tempfile::tempdir().unwrap();
        let bound = b"def alpha():\r\n    return 1\r\n".to_vec();
        let other = b"def otherfn():\r\n    return 7\r\n".to_vec();
        let mut tampered = {
            let mut metadata = engram_domain::Metadata::new();
            metadata.insert(
                "codeGenerationId".to_owned(),
                serde_json::Value::String("gen-1".to_owned()),
            );
            metadata.insert(
                "codeRepoId".to_owned(),
                serde_json::Value::String("bound-repo".to_owned()),
            );
            engram_domain::KnowledgeChunk {
                id: engram_domain::Id::from("chunk-path-tampered"),
                document_id: engram_domain::Id::from("doc-path"),
                source_id: engram_domain::Id::from("src-path"),
                kind: KnowledgeChunkKind::CodeSymbol,
                // The row's TRUE text (a declaration of ANOTHER file)...
                text: "def otherfn():".to_owned(),
                summary: None,
                // ...with a CHANGED path claiming THIS file.
                location: Some(engram_domain::SourceLocation {
                    path: Some("src/bound.py".to_owned()),
                    start_line: Some(1),
                    end_line: Some(2),
                    start_offset: None,
                    end_offset: None,
                    anchor: None,
                }),
                entities: vec![engram_domain::EntityRef {
                    id: Some(engram_domain::Id::from("chunk-path-tampered")),
                    kind: None,
                    name: Some("otherfn".to_owned()),
                    aliases: Vec::new(),
                }],
                concepts: Vec::new(),
                embedding_refs: Vec::new(),
                content_hash: "sha256:chunk-path-tampered".to_owned(),
                provenance: fixture_provenance(),
                policy: fixture_policy(),
                created_at: chrono::Utc::now(),
                updated_at: None,
                metadata: Some(metadata),
            }
        };
        tampered.location = Some(engram_domain::SourceLocation {
            path: Some("src/bound.py".to_owned()),
            start_line: Some(1),
            end_line: Some(2),
            start_offset: None,
            end_offset: None,
            anchor: None,
        });
        let mut legit = tampered.clone();
        legit.id = engram_domain::Id::from("chunk-path-real");
        legit.entities = vec![engram_domain::EntityRef {
            id: Some(engram_domain::Id::from("chunk-path-real")),
            kind: None,
            name: Some("alpha".to_owned()),
            aliases: Vec::new(),
        }];
        legit.text = "def alpha():".to_owned();
        let (app, _root, _bindings) = bound_app_with_rows(
            dir.path(),
            vec![
                CommittedFile {
                    path: "src/bound.py".into(),
                    oid: "b".repeat(40),
                    mode: "100644".to_owned(),
                    bytes: bound,
                },
                CommittedFile {
                    path: "src/other.py".into(),
                    oid: "c".repeat(40),
                    mode: "100644".to_owned(),
                    bytes: other,
                },
            ],
            vec![legit, tampered],
            &[("chunk-path-real".to_owned(), "gen-1".to_owned(), 1, 2)],
        );
        // The tampered row's DURABLE canonical path is the OTHER FILE — its
        // mutable location claims this path, but the durable association
        // says src/other.py (final r5 finding 2: repository/generation
        // match does not authenticate path identity).
        _bindings.with_chunk_lineage_at_path(&[(
            "chunk-path-tampered".to_owned(),
            "gen-1".to_owned(),
            "src/other.py".to_owned(),
            Some((1, 2)),
        )]);
        let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            err.message.contains("canonical source path differs"),
            "a changed path on a canonical-ID row denies: {}",
            err.message
        );
    }

    /// A4 (r5, the publication-at-selection/read barrier): a REAL
    /// publication deterministically interposed between participant
    /// selection and the byte read (inside the consistent-read window) can
    /// never produce mixed-generation output — the window's post-check
    /// catches it, the retry REBUILDS the participant set against the new
    /// view, and the rebuilt attempt denies (the selected generation is no
    /// longer published). The flip is a store double state change at a named
    /// seam — no threads, no timing.
    #[test]
    fn publication_between_selection_and_read_denies_never_mixed() {
        let dir = tempfile::tempdir().unwrap();
        let source = "def alpha():\r\n    return 1\r\n";
        let (app, _root, bindings) =
            bound_app_impl(dir.path(), source.as_bytes().to_vec(), "alpha", 1, 2);
        bindings.arm_descriptor_flip(
            1,
            CodeGeneration {
                id: "gen-3".to_owned(),
                binding_version: 1,
                commit_oid: "9".repeat(40),
                snapshot_digest: "d".repeat(64),
                parser_fingerprint: "parser".to_owned(),
                filter_fingerprint: "filter".to_owned(),
            },
        );
        let err = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            err.message.contains("no current canonical participants"),
            "the rebuilt attempt denies instead of serving the retired \
             generation's bytes: {}",
            err.message
        );
        assert!(
            bindings.flip_fired(),
            "the publication barrier actually fired inside the window"
        );
        // A follow-up read on the same state denies consistently — there is
        // no mixed state left behind.
        let again = fetch_content(&app, &json!({ "path": "src/bound.py" })).unwrap_err();
        assert!(
            again.message.contains("no current canonical participants"),
            "the post-publication state denies consistently: {}",
            again.message
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
        // The file has TWO lines (it ends with CRLF); the claim spans its
        // true lines 1-2 — final r5 finding 2 rejects out-of-range span
        // ends instead of clamping them to EOF.
        let (app, _retained) = bound_app_with_symbols_bytes(dir.path(), &source_bytes, 1, 2);
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
