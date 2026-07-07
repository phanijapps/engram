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

const DENY_DIRS: &[&str] = &[
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
const DENY_FILE_EXT: &[&str] = &["db", "sqlite", "sqlite3", "node", "log", "pyc", "lock"];
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
pub fn is_denylisted(rel_path: &str) -> bool {
    let segs: Vec<&str> = rel_path.split(['/', '\\']).collect();
    if segs.iter().any(|s| DENY_DIRS.contains(s)) {
        return true;
    }
    let base = segs.last().copied().unwrap_or("");
    let ext = match base.rsplit_once('.') {
        Some((_, e)) => e.to_lowercase(),
        None => String::new(),
    };
    DENY_FILE_EXT.iter().any(|e| *e == ext)
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
