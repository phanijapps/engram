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

/// The bound-code origin `(repo_id, generation_id)` of a chunk set: present
/// exactly when the chunks carry the scanner's generation stamps (task_017).
/// Non-code / unbound documents return `None` and keep the historical read
/// paths.
fn bound_code_origin(chunks: &[&engram_domain::KnowledgeChunk]) -> Option<(String, String)> {
    let chunk = chunks.iter().find(|chunk| {
        chunk
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("codeGenerationId"))
            .is_some()
    })?;
    let metadata = chunk.metadata.as_ref()?;
    let repo = metadata
        .get("codeRepoId")
        .and_then(|value| value.as_str())?
        .to_owned();
    let generation = metadata
        .get("codeGenerationId")
        .and_then(|value| value.as_str())?
        .to_owned();
    Some((repo, generation))
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
    // Chunks stamped with a code generation serve the retained snapshot of
    // THAT generation's commit — byte-exact, revision-labeled, and with NO
    // working-tree fallback: a missing snapshot is an error in auto and
    // verbatim modes alike (REQ-006: exact content or an explicit
    // unavailable error, never a checkout masquerading as the revision).
    if mode != "chunks"
        && let Some((repo_id, generation_id)) = bound_code_origin(&ordered)
    {
        let exact = futures::executor::block_on(app.provider.code_fetch_exact_at(
            &app.scope,
            &repo_id,
            &generation_id,
            &path,
            None,
        ));
        return match exact {
            Ok(view) => {
                let total_bytes = view.bytes.len();
                let content = String::from_utf8_lossy(&view.bytes).into_owned();
                let (content, truncated) = truncate_bytes(&content, max_bytes);
                Ok(protocol::text_content(
                    json!({
                        "path": path,
                        "repository": repo_id,
                        "kind": kind,
                        "source": "exact",
                        "revision": view.commit_oid,
                        "generation": generation_id,
                        "span": view.span.map(|(start, end)| json!({ "start": start, "end": end })),
                        "content_sha256": view.served_digest,
                        "chunk_count": ordered.len(),
                        "total_bytes": total_bytes,
                        "returned_bytes": content.len(),
                        "max_bytes": max_bytes,
                        "truncated": truncated,
                        "content": content,
                        "note": "exact retained committed bytes for the indexed revision (no checkout fallback)",
                    })
                    .to_string(),
                ))
            }
            Err(error) => Err(ToolError::new(
                -32603,
                format!("exact retained source unavailable: {error}"),
            )),
        };
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
    use engram_ingest::git_source::{CommittedFile, ResolvedCommit, retain_snapshot};
    use engram_integration::EngramProviderBuilder;
    use engram_integration::code_binding::{
        CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
    };
    use engram_knowledge::{CodeBindingRepository, KnowledgeRepository};
    use engram_store_sqlite::knowledge::{SqlCodeBindingRepository, SqlKnowledgeStore};

    struct PermissiveAuthorizer;
    #[async_trait::async_trait]
    impl CodeBindingAuthorizer for PermissiveAuthorizer {
        async fn authorize(
            &self,
            _actor: &engram_domain::Actor,
            _scope: &engram_domain::Scope,
            _repo: &engram_domain::RepoId,
            _action: CodeAuthorityAction,
        ) -> engram_knowledge::CoreResult<()> {
            Ok(())
        }
    }

    /// Builds a BOUND MCP app over a real retained snapshot: repo bound +
    /// published at `gen-1` whose commit OID owns the retained bytes, with
    /// knowledge chunks stamped for that generation. Returns the app plus the
    /// retained root for deletion experiments.
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

    fn bound_app(dir: &std::path::Path, source: &str) -> (App, std::path::PathBuf) {
        use futures::executor::block_on;
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
            bytes: source.as_bytes().to_vec(),
        }];
        // The retention stage creates and owns its private storage root
        // itself (0700, no links) — never pre-create it.
        let retained_root = dir.join("retained");
        let snapshot = retain_snapshot(&retained_root, commit, &files).expect("retain snapshot");

        // Durable binding store: bind + publish gen-1 at that commit.
        let bindings = SqlCodeBindingRepository::open(dir.join("bound-bindings.sqlite")).unwrap();
        block_on(bindings.bind(engram_domain::CodeBinding {
            scope: scope.clone(),
            repo_id: engram_domain::RepoId("bound-repo".to_owned()),
            branch_ref: "refs/heads/main".to_owned(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .unwrap();
        let repo_id = engram_domain::RepoId("bound-repo".to_owned());
        let generation = CodeGeneration {
            id: "gen-1".to_owned(),
            binding_version: 1,
            commit_oid: snapshot.commit.oid.clone(),
            snapshot_digest: snapshot.digest.clone(),
            parser_fingerprint: "parser".to_owned(),
            filter_fingerprint: "filter".to_owned(),
        };
        let attempt = engram_domain::CodeAttempt {
            id: "attempt-fetch".to_owned(),
            idempotency_key: "idempotent-fetch".to_owned(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 0,
            state: engram_domain::CodeAttemptState::Created,
            receipt_kinds: vec![
                engram_domain::CodeReceiptKind::Source,
                engram_domain::CodeReceiptKind::Knowledge,
            ],
        };
        block_on(bindings.begin_attempt(&scope, &repo_id, generation.clone(), attempt)).unwrap();
        for lane in [
            engram_domain::CodeReceiptKind::Source,
            engram_domain::CodeReceiptKind::Knowledge,
        ] {
            block_on(
                bindings.record_receipt(engram_knowledge::StageReceipt {
                    attempt_id: "attempt-fetch".to_owned(),
                    generation_id: "gen-1".to_owned(),
                    lane: serde_json::to_value(&lane)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    digest: "d".repeat(64),
                }),
            )
            .unwrap();
        }
        let (_, stored) = block_on(bindings.get_attempt(&scope, &repo_id, "attempt-fetch"))
            .unwrap()
            .unwrap();
        block_on(bindings.mark_ready(&scope, &repo_id, "attempt-fetch", stored.fencing_token))
            .unwrap();
        block_on(bindings.publish(&scope, &repo_id, "attempt-fetch", 0)).unwrap();
        let bindings = std::sync::Arc::new(bindings);

        // Knowledge store with one bound code chunk stamped for gen-1.
        let knowledge = std::sync::Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
        block_on(knowledge.put_source(engram_domain::KnowledgeSource {
            id: engram_domain::Id::from("src-bound"),
            kind: engram_domain::SourceKind::GitRepository,
            scope: scope.clone(),
            name: "bound-repo".to_owned(),
            uri: None,
            version: None,
            repository_key: Some("bound-repo".to_owned()),
            git_ref: Some("refs/heads/main".to_owned()),
            policy: fixture_policy(),
            provenance: fixture_provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }))
        .unwrap();
        block_on(knowledge.put_document(SourceDocument {
            id: engram_domain::Id::from("doc-bound"),
            source_id: engram_domain::Id::from("src-bound"),
            kind: SourceDocumentKind::Code,
            uri: None,
            path: Some("src/bound.py".to_owned()),
            title: None,
            mime_type: None,
            language: Some("python".to_owned()),
            version: None,
            content_hash: "sha256:bound".to_owned(),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: None,
        }))
        .unwrap();
        let mut stamps = engram_domain::Metadata::new();
        stamps.insert(
            "codeGenerationId".to_owned(),
            serde_json::Value::String("gen-1".to_owned()),
        );
        stamps.insert(
            "codeRepoId".to_owned(),
            serde_json::Value::String("bound-repo".to_owned()),
        );
        block_on(knowledge.put_chunk(engram_domain::KnowledgeChunk {
            id: engram_domain::Id::from("chunk-bound-1"),
            document_id: engram_domain::Id::from("doc-bound"),
            source_id: engram_domain::Id::from("src-bound"),
            kind: KnowledgeChunkKind::CodeSymbol,
            text: source.lines().next().unwrap_or_default().to_owned(),
            summary: None,
            location: Some(engram_domain::SourceLocation {
                path: Some("src/bound.py".to_owned()),
                start_line: Some(1),
                end_line: Some(1),
                start_offset: None,
                end_offset: None,
                anchor: None,
            }),
            entities: Vec::new(),
            concepts: Vec::new(),
            embedding_refs: Vec::new(),
            content_hash: "sha256:chunk-bound-1".to_owned(),
            provenance: fixture_provenance(),
            policy: fixture_policy(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            metadata: Some(stamps),
        }))
        .unwrap();

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
        .code_source(std::sync::Arc::new(
            engram_integration::code_binding::RetainedCodeSource::new(retained_root.clone()),
        ))
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
