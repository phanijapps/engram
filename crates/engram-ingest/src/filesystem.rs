//! Local filesystem source reader.
//!
//! This module owns bounded file discovery and UTF-8 reads for local source
//! trees. It implements the `SourceReader` port without persisting knowledge,
//! following symlinks, parsing Git history, or deriving code symbols.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use async_trait::async_trait;
use chrono::Utc;
use engram_core::{CoreError, CoreResult, SourceReader};
use engram_domain::{
    KnowledgeSource, SourceDocument, SourceDocumentKind, SourceKind, SourceLocation,
};

use crate::content_hash;

const DEFAULT_MAX_FILE_BYTES: u64 = 1_048_576;

/// Configuration for local filesystem source discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemSourceReaderOptions {
    pub max_file_bytes: u64,
    pub include_hidden: bool,
}

impl Default for FilesystemSourceReaderOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            include_hidden: false,
        }
    }
}

/// Source reader for text, Markdown, and code files under one root directory.
///
/// The root is a trust boundary. Documents are exposed with relative paths, and
/// later reads reject absolute or parent-relative paths before touching disk.
#[derive(Debug, Clone)]
pub struct FilesystemSourceReader {
    root: PathBuf,
    options: FilesystemSourceReaderOptions,
}

impl FilesystemSourceReader {
    /// Creates a reader rooted at a local directory.
    ///
    /// The directory must exist when the reader is constructed. Symlinks are not
    /// followed during discovery so callers can reason about the configured root
    /// as the only readable tree.
    pub fn new(
        root: impl Into<PathBuf>,
        options: FilesystemSourceReaderOptions,
    ) -> CoreResult<Self> {
        if options.max_file_bytes == 0 {
            return invalid("max_file_bytes must be greater than zero");
        }
        let root = root.into();
        let metadata = fs::metadata(&root).map_err(file_error)?;
        if !metadata.is_dir() {
            return invalid("filesystem source root must be a directory");
        }
        Ok(Self { root, options })
    }

    fn discover_documents(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>> {
        if source.kind != SourceKind::Filesystem {
            return invalid("filesystem reader requires SourceKind::Filesystem");
        }

        let mut files = Vec::new();
        self.collect_files(&self.root, &mut files)?;
        files.sort();

        files
            .into_iter()
            .map(|path| self.document_for_path(source, &path))
            .collect()
    }

    fn collect_files(&self, directory: &Path, files: &mut Vec<PathBuf>) -> CoreResult<()> {
        let mut entries = fs::read_dir(directory)
            .map_err(file_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(file_error)?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type().map_err(file_error)?;
            if should_skip_path(&path, self.options.include_hidden) || file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                self.collect_files(&path, files)?;
                continue;
            }
            if file_type.is_file() && classify_document(&path).is_some() {
                files.push(path);
            }
        }
        Ok(())
    }

    fn document_for_path(
        &self,
        source: &KnowledgeSource,
        path: &Path,
    ) -> CoreResult<SourceDocument> {
        let relative_path = self.relative_path(path)?;
        let bytes = self.read_bytes(&relative_path)?;
        let content_hash = content_hash(&bytes);
        let kind = classify_document(path).expect("classified before document construction");
        let now = Utc::now();

        Ok(SourceDocument {
            id: source_document_id(&source.id.to_string(), &relative_path, &content_hash),
            source_id: source.id.clone(),
            kind,
            uri: None,
            path: Some(relative_path.clone()),
            title: path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            mime_type: mime_type(path).map(str::to_owned),
            language: language(path).map(str::to_owned),
            version: source.version.clone(),
            content_hash,
            provenance: source.provenance.clone(),
            policy: source.policy.clone(),
            created_at: now,
            updated_at: None,
            metadata: Some(
                [(
                    "location".to_owned(),
                    serde_json::to_value(SourceLocation {
                        path: Some(relative_path),
                        start_line: None,
                        end_line: None,
                        start_offset: None,
                        end_offset: None,
                        anchor: None,
                    })
                    .map_err(|error| CoreError::Adapter {
                        adapter: "engram-ingest-filesystem".to_owned(),
                        message: error.to_string(),
                    })?,
                )]
                .into_iter()
                .collect(),
            ),
        })
    }

    fn read_bytes(&self, relative_path: &str) -> CoreResult<Vec<u8>> {
        read_file_bytes(&self.root, &self.options, relative_path)
    }

    fn relative_path(&self, path: &Path) -> CoreResult<String> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| CoreError::InvalidRequest {
                reason: "document path is outside filesystem source root".to_owned(),
            })?;
        normalize_relative_path(relative)
    }
}

#[async_trait]
impl SourceReader for FilesystemSourceReader {
    async fn read_source(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>> {
        self.discover_documents(source)
    }

    async fn read_document(&self, document: &SourceDocument) -> CoreResult<String> {
        let path = document
            .path
            .as_deref()
            .ok_or_else(|| CoreError::InvalidRequest {
                reason: "document.path is required for filesystem reads".to_owned(),
            })?;
        let bytes = self.read_bytes(path)?;
        String::from_utf8(bytes).map_err(|error| CoreError::InvalidRequest {
            reason: format!("document is not valid UTF-8: {error}"),
        })
    }
}

/// Reads one relative file after enforcing shared source-reader safety checks.
///
/// Git and filesystem readers both use this path so max-size, symlink, regular
/// file, and traversal behavior stay identical across local source adapters.
pub(crate) fn read_file_bytes(
    root: &Path,
    options: &FilesystemSourceReaderOptions,
    relative_path: &str,
) -> CoreResult<Vec<u8>> {
    validate_relative_path(relative_path)?;
    let path = root.join(relative_path);
    let metadata = fs::symlink_metadata(&path).map_err(file_error)?;
    if !metadata.is_file() {
        return invalid("document path must point to a regular file");
    }
    if metadata.file_type().is_symlink() {
        return invalid("document path must not be a symlink");
    }
    if metadata.len() > options.max_file_bytes {
        return invalid(format!(
            "document exceeds max_file_bytes: {} > {}",
            metadata.len(),
            options.max_file_bytes
        ));
    }
    fs::read(path).map_err(file_error)
}

/// Validates that a document path is relative and contains no traversal.
///
/// This protects source readers before they join caller-provided paths onto a
/// trusted root directory.
pub(crate) fn validate_relative_path(path: &str) -> CoreResult<()> {
    if path.trim().is_empty() {
        return invalid("document path must not be empty");
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return invalid("document path must be relative");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return invalid("document path must not contain traversal components"),
        }
    }
    Ok(())
}

/// Converts a filesystem-relative path into the portable slash-separated form.
///
/// The returned value is safe to store on `SourceDocument.path` because it has
/// already rejected prefixes, parent components, and non-UTF-8 segments.
pub(crate) fn normalize_relative_path(path: &Path) -> CoreResult<String> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(
                value
                    .to_str()
                    .ok_or_else(|| CoreError::InvalidRequest {
                        reason: "document path must be valid UTF-8".to_owned(),
                    })?
                    .to_owned(),
            ),
            _ => return invalid("document path must not contain traversal components"),
        }
    }
    Ok(components.join("/"))
}

fn should_skip_path(path: &Path, include_hidden: bool) -> bool {
    if include_hidden {
        return false;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// Classifies supported first-slice text, structured-data, and code files.
///
/// Unsupported extensions return `None` so readers can skip binary and unknown
/// files without pretending they are source-grounded text.
pub(crate) fn classify_document(path: &Path) -> Option<SourceDocumentKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "txt" => Some(SourceDocumentKind::Text),
        "md" | "markdown" => Some(SourceDocumentKind::Markdown),
        "html" | "htm" => Some(SourceDocumentKind::Html),
        "json" | "jsonl" | "csv" | "tsv" => Some(SourceDocumentKind::StructuredData),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "kt" | "swift" | "c" | "cc"
        | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "scala" | "sh" | "bash" | "zsh" | "fish"
        | "toml" | "yaml" | "yml" => Some(SourceDocumentKind::Code),
        _ => None,
    }
}

/// Returns a conservative MIME hint for supported local source files.
///
/// The value is metadata only; callers must not infer parser or security policy
/// from it.
pub(crate) fn mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "txt" => Some("text/plain"),
        "md" | "markdown" => Some("text/markdown"),
        "html" | "htm" => Some("text/html"),
        "json" | "jsonl" => Some("application/json"),
        "csv" => Some("text/csv"),
        "tsv" => Some("text/tab-separated-values"),
        _ => Some("text/plain"),
    }
}

/// Returns a lightweight language hint from a supported file extension.
///
/// This is not symbol extraction or syntax validation; it only gives downstream
/// chunkers and examples a stable hint.
pub(crate) fn language(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" => Some("kotlin"),
        "swift" => Some("swift"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "hpp" => Some("cpp"),
        "cs" => Some("csharp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "scala" => Some("scala"),
        "sh" | "bash" | "zsh" | "fish" => Some("shell"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "json" | "jsonl" => Some("json"),
        "md" | "markdown" => Some("markdown"),
        _ => None,
    }
}

/// Builds a deterministic document ID from source, relative path, and content.
///
/// The path participates in identity so two files with identical content remain
/// distinct documents inside the same source.
pub(crate) fn source_document_id(
    source_id: &str,
    relative_path: &str,
    document_hash: &str,
) -> engram_domain::DocumentId {
    engram_domain::Id::from(format!(
        "document-{}",
        content_hash(format!(
            "{source_id}\u{1f}{relative_path}\u{1f}{document_hash}"
        ))
        .trim_start_matches("sha256:")
    ))
}

fn invalid<T>(reason: impl Into<String>) -> CoreResult<T> {
    Err(CoreError::InvalidRequest {
        reason: reason.into(),
    })
}

/// Translates filesystem I/O failures into the stable core adapter error.
///
/// Source readers keep raw I/O types behind this boundary so callers can handle
/// portable `CoreError` categories.
pub(crate) fn file_error(error: std::io::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-ingest-filesystem".to_owned(),
        message: error.to_string(),
    }
}
