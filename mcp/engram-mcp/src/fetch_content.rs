//! `fetch_content`: stitch one indexed file's content back from stored chunks.
//!
//! Given a path, returns the stitched content: doc files (Markdown/text) are
//! reassembled from their document-section chunks; code files are reassembled
//! from their symbol chunks (best-effort — see the code note below). Content
//! is capped at [`DEFAULT_MAX_BYTES`] (100 KiB) by default; pass `max_bytes`
//! to raise/lower it within [`MAX_BYTES_CEILING`].
//!
//! Scope v1 (edge cases deliberately deferred): reads every chunk in the
//! scope via [`KnowledgeQuery::list_chunks`] and filters by
//! `location.path` — correct everywhere, but not yet a lean per-path port
//! method. Multi-repo scopes disambiguate via the `repository` filter.

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

/// The `fetch_content` tool handler.
pub fn fetch_content(app: &App, args: &Value) -> Result<Value, ToolError> {
    let path = normalize_path(req_str(args, "path")?);
    let repository = args.get("repository").and_then(|v| v.as_str());
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

    // Stitch: skip graph-only anchors (empty text — containers with neither
    // doc comment nor signature). Docs join with a blank line (section
    // boundaries); code joins with a single newline (line spans are
    // contiguous-ish).
    let separator = if is_code { "\n" } else { "\n\n" };
    let parts: Vec<&str> = ordered
        .iter()
        .filter(|c| !c.text.is_empty())
        .map(|c| c.text.as_str())
        .collect();
    let full = parts.join(separator);
    let total_bytes = full.len();
    let (content, truncated) = truncate_bytes(&full, max_bytes);

    let first_line = ordered
        .iter()
        .find_map(|c| c.location.as_ref().and_then(|l| l.start_line));
    let last_line = ordered
        .iter()
        .rev()
        .find_map(|c| c.location.as_ref().and_then(|l| l.end_line));

    let mut body = json!({
        "path": path,
        "repository": sources[0],
        "kind": kind,
        "chunk_count": ordered.len(),
        "lines": { "first": first_line, "last": last_line },
        "total_bytes": total_bytes,
        "returned_bytes": content.len(),
        "max_bytes": max_bytes,
        "truncated": truncated,
        "content": content,
    });
    if is_code {
        body["note"] = json!(
            "code stitching is best-effort on this build: container chunks store \
             doc-comment + signature only, and interstitial lines (imports, stray \
             comments) are not chunked — treat as a reconstruction, not exact source"
        );
    }
    Ok(protocol::text_content(body.to_string()))
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
