//! File classification logic for repository scanning.
//!
//! Determines whether files should be included in scans based on denylists,
//! secret file detection, and file kind classification (code vs text).

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Code,
    Text,
}

pub(crate) const DENY_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    ".fastembed_cache",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".cache",
    ".idea",
    ".vscode",
];
pub(crate) const DENY_FILE_EXT: &[&str] =
    &["db", "sqlite", "sqlite3", "node", "log", "pyc", "lock"];
const SECRET_EXT: &[&str] = &[".key", ".pem", ".cert", ".crt", ".p12", ".pfx"];
const SECRET_NAMES: &[&str] = &["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];
const SAFE_TEMPLATES: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.defaults",
    ".env.schema",
];
const CODE_NAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "rakefile",
    "gemfile",
    "cmake",
    "justfile",
];
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "scala", "clj",
    "cljs", "ex", "exs", "erl", "hs", "ml", "mli", "lua", "php", "pl", "pm", "r", "rb", "sh",
    "bash", "zsh", "fish", "ps1", "c", "h", "cpp", "cc", "cxx", "hpp", "hxx", "cs", "swift",
    "dart", "vue", "svelte", "sql", "proto", "graphql", "gradle", "groovy", "vim",
    // Salesforce: Apex classes (`.cls`) + triggers (`.trigger`) — the sfapex
    // grammar is registered in the tree-sitter chunker but these were never
    // routed to the code path, so Apex source was silently un-indexed
    // (dreamhouse-lwc: 0 `.cls` documents; the Apex surface only appeared via
    // LWC `@salesforce/apex` imports).
    "cls", "apex", "trigger",
];
const TEXT_EXTENSIONS: &[&str] = &[
    "md",
    "markdown",
    "txt",
    "rst",
    "org",
    "tex",
    "adoc",
    "yml",
    "yaml",
    "json",
    "toml",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "ini",
    "cfg",
    "conf",
    "properties",
    "csv",
    "tsv",
];

fn file_base(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// True if any path segment is a deny dir, or the file suffix is denylisted.
///
/// Delegates to the builtin [`crate::scan_filter::ScanFilter`] so the decision
/// logic has a single source of truth; the `DENY_DIRS`/`DENY_FILE_EXT` consts
/// remain here as that filter's builtin input.
pub fn is_denylisted(rel_path: &str) -> bool {
    crate::scan_filter::ScanFilter::builtin().is_denylisted(rel_path)
}

/// True if the file name looks like a credential/secret carrier.
pub fn is_secret_file(name: &str) -> bool {
    let base = file_base(name).to_lowercase();
    if SAFE_TEMPLATES.iter().any(|t| *t == base) {
        return false;
    }
    if base == ".env" || base.starts_with(".env.") {
        return true;
    }
    if SECRET_EXT.iter().any(|e| base.ends_with(e)) {
        return true;
    }
    SECRET_NAMES.iter().any(|n| *n == base)
}

/// Classify a file by name; `None` means "not included".
pub fn classify_file(name: &str) -> Option<FileKind> {
    let base = file_base(name).to_lowercase();
    if CODE_NAMES.iter().any(|n| *n == base) {
        return Some(FileKind::Code);
    }
    let ext = match base.rsplit_once('.') {
        Some((_, e)) => e,
        None => "",
    };
    if CODE_EXTENSIONS.contains(&ext) {
        return Some(FileKind::Code);
    }
    if TEXT_EXTENSIONS.contains(&ext) {
        return Some(FileKind::Text);
    }
    None
}

/// True if `target` is `root` or inside it (callers canonicalize both first).
pub fn is_within_root(target: &Path, root: &Path) -> bool {
    target.starts_with(root)
}

/// code-graph-quality [minified-vendor-noise]: minified/bundled asset NAME
/// patterns — stable, non-transient filter signals. Measured motivation: 9
/// vendored bundles injected 372 single/double-char entities and 37% of all
/// `calls` edges in the spring-boot-demo store.
pub fn looks_minified_name(rel_path: &str) -> bool {
    let base = file_base(rel_path).to_lowercase();
    if base.ends_with(".min.js") || base.ends_with(".min.css") || base.ends_with(".bundle.js") {
        return true;
    }
    // Source maps are pure build output — never source.
    base.ends_with(".js.map") || base.ends_with(".css.map") || base == "bundle.js"
}

/// code-graph-quality [minified-vendor-noise]: minified/bundled asset CONTENT
/// heuristic — bundles with innocuous names are caught by shape: few, very
/// long lines. A source file with 40+ average bytes per line across its whole
/// body is not human-written (real code averages 10-35). Small files (< 4 KiB)
/// are exempt so one-liner configs never trip it.
pub fn looks_minified_bytes(bytes: &[u8]) -> bool {
    const MIN_SIZE: usize = 4 * 1024;
    const AVG_LINE_BYTES: usize = 400;
    if bytes.len() < MIN_SIZE {
        return false;
    }
    let lines = bytes.iter().filter(|b| **b == b'\n').count().max(1);
    bytes.len() / lines > AVG_LINE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylisted_dirs_and_extensions() {
        assert!(is_denylisted("node_modules/package.json"));
        assert!(is_denylisted("target/debug/lib.rs"));
        assert!(is_denylisted("file.db"));
        assert!(is_denylisted("config.log"));
        assert!(!is_denylisted("src/main.rs"));
    }

    #[test]
    fn secret_files_detected_but_templates_safe() {
        assert!(is_secret_file(".env"));
        assert!(is_secret_file("id_rsa"));
        assert!(is_secret_file("cert.pem"));
        assert!(!is_secret_file(".env.example"));
        assert!(!is_secret_file(".env.template"));
    }

    #[test]
    fn classify_by_name_and_extension() {
        assert_eq!(classify_file("dockerfile"), Some(FileKind::Code));
        assert_eq!(classify_file("src/main.rs"), Some(FileKind::Code));
        assert_eq!(classify_file("README.md"), Some(FileKind::Text));
        assert_eq!(classify_file("data.json"), Some(FileKind::Text));
        assert_eq!(classify_file("image.png"), None);
    }
}
