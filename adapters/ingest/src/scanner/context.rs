//! Scan phase 1.5: repo-context collection (receiver-aware call resolution,
//! plan v6 §4).
//!
//! Builds the repo-wide [`RepoContext`] (method return evidence, anchor
//! counts, known types/modules, struct fields) from EVERY eligible code file
//! — changed AND unchanged — before the unchanged-skip decision. Receiver
//! inference depends on declarations in files that may not have changed, so
//! evidence collection must see the whole repo. Streaming: trees are parsed
//! one file at a time and never retained.

use std::path::Path;

use engram_code::parser::repo_context::RepoContext;
use engram_code::parser::repo_scan::collect_evidence;

use super::walk::Readable;
use crate::classifier::FileKind;

/// Extension → language id for evidence collection (mirrors the chunker's
/// supported set; unsupported extensions are skipped).
fn ext_of(rel: &str) -> Option<String> {
    let ext = rel.rsplit('.').next()?.to_ascii_lowercase();
    const SUPPORTED: &[&str] = &["rs", "ts", "tsx", "js", "jsx", "mts", "cts", "java"];
    if SUPPORTED.contains(&ext.as_str()) {
        Some(ext)
    } else {
        None
    }
}

/// Collects repo evidence from all readable code files.
///
/// Best-effort per file: read or parse failures skip the file (missing
/// evidence degrades to Unknown at lookup — never a false edge).
pub(super) fn collect_repo_context(
    readable: &[Readable],
    ts_chunker: Option<&crate::TreeSitterChunker>,
) -> RepoContext {
    let Some(ts) = ts_chunker else {
        return RepoContext::empty();
    };
    let mut builder = RepoContext::builder();
    for r in readable {
        if r.kind != FileKind::Code {
            continue;
        }
        let Some(ext) = ext_of(&r.rel) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(Path::new(&r.canonical)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if let Ok(tree) = ts.parse(&text, &ext) {
            collect_evidence(tree.root_node(), text.as_bytes(), &ext, &[], &mut builder);
        }
    }
    builder.build()
}
