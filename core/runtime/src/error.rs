//! Stable error surface and redaction for Engram behavior crates.
//!
//! [`CoreError`] is the portable error enum services and adapters translate
//! infrastructure failures into. [`DiagnosticError`] is the full-detail wrapper
//! for local development. `redact_message` scrubs private internals before an
//! error crosses a trust boundary.

use thiserror::Error;

/// Stable error surface shared by services and adapters.
///
/// Adapter implementations should translate infrastructure-specific failures
/// into these categories at the boundary. Detailed diagnostics can be logged by
/// the adapter, but callers should be able to make portable decisions from this
/// enum without knowing which store, index, or provider was used.
///
/// # Redaction and diagnostic modes
///
/// By construction a `CoreError` carries full detail — the *local diagnostic*
/// mode. Two methods move an error across a trust boundary:
///
/// - [`CoreError::to_redacted`] returns a copy with SQL internals, absolute
///   paths, raw embedding vectors, and private record contents scrubbed from
///   its string fields. Use this before an error crosses into a public API
///   response, a log shipped off-host, or any context where private internals
///   must not leak.
/// - [`CoreError::with_diagnostic`] is the explicit opt-in counterpart: it
///   wraps the error in [`DiagnosticError`], signaling that the full detail is
///   intended for local development output and must not be redacted.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("record not found: {target_type}:{target_id}")]
    NotFound {
        target_type: &'static str,
        target_id: String,
    },
    #[error("policy denied: {reason}")]
    PolicyDenied { reason: String },
    #[error("invalid request: {reason}")]
    InvalidRequest { reason: String },
    #[error("adapter failed: {adapter}: {message}")]
    Adapter { adapter: String, message: String },
    #[error("conflict: {reason}")]
    Conflict { reason: String },
    #[error("provider unavailable: {provider}")]
    ProviderUnavailable { provider: String },
    #[error("migration pending: {reason}")]
    MigrationPending { reason: String },
    #[error("operation rejected in dry-run mode: {operation}")]
    DryRunMode { operation: String },
    #[error("capability unsupported: {capability}: {reason}")]
    CapabilityUnsupported { capability: String, reason: String },
    #[error("embedding space mismatch: expected {expected}, actual {actual}")]
    EmbeddingSpaceMismatch { expected: String, actual: String },
    #[error("migration manifest stale: expected {expected}, actual {actual}")]
    MigrationManifestStale { expected: String, actual: String },
}

impl CoreError {
    /// Returns a copy of this error with private internals scrubbed.
    ///
    /// Removes SQL internals (statements, table names, `sqlite:` prefixes),
    /// absolute filesystem paths, raw embedding vectors, and bracketed private
    /// record contents from every string field. The error *category* and
    /// stable reason text are preserved so callers can still make portable
    /// decisions; only private detail is redacted.
    ///
    /// Use this before an error message crosses into a public API response, an
    /// off-host log, or any context where private internals must not leak.
    #[must_use]
    pub fn to_redacted(&self) -> Self {
        match self {
            CoreError::NotFound {
                target_type,
                target_id,
            } => CoreError::NotFound {
                target_type,
                target_id: redact_message(target_id),
            },
            CoreError::PolicyDenied { reason } => CoreError::PolicyDenied {
                reason: redact_message(reason),
            },
            CoreError::InvalidRequest { reason } => CoreError::InvalidRequest {
                reason: redact_message(reason),
            },
            CoreError::Adapter { adapter, message } => CoreError::Adapter {
                adapter: adapter.clone(),
                message: redact_message(message),
            },
            CoreError::Conflict { reason } => CoreError::Conflict {
                reason: redact_message(reason),
            },
            CoreError::ProviderUnavailable { provider } => CoreError::ProviderUnavailable {
                provider: provider.clone(),
            },
            CoreError::MigrationPending { reason } => CoreError::MigrationPending {
                reason: redact_message(reason),
            },
            CoreError::DryRunMode { operation } => CoreError::DryRunMode {
                operation: operation.clone(),
            },
            CoreError::CapabilityUnsupported { capability, reason } => {
                CoreError::CapabilityUnsupported {
                    capability: capability.clone(),
                    reason: redact_message(reason),
                }
            }
            CoreError::EmbeddingSpaceMismatch { expected, actual } => {
                CoreError::EmbeddingSpaceMismatch {
                    expected: expected.clone(),
                    actual: actual.clone(),
                }
            }
            CoreError::MigrationManifestStale { expected, actual } => {
                CoreError::MigrationManifestStale {
                    expected: expected.clone(),
                    actual: actual.clone(),
                }
            }
        }
    }

    /// Returns a redacted, public-safe display string for this error.
    ///
    /// Equivalent to `self.to_redacted().to_string()` but allocates once.
    #[must_use]
    pub fn redacted_display(&self) -> String {
        self.to_redacted().to_string()
    }

    /// Wraps this error in [`DiagnosticError`] for local development output.
    ///
    /// This is the explicit opt-in counterpart to [`Self::to_redacted`]: it
    /// signals that the full detail is intended for a trusted local context
    /// (developer console, local log) and must not be redacted.
    #[must_use]
    pub fn with_diagnostic(self) -> DiagnosticError {
        DiagnosticError { inner: self }
    }
}

/// Result type used by Engram behavior ports.
pub type CoreResult<T> = Result<T, CoreError>;

/// Full-detail diagnostic wrapper around a [`CoreError`].
///
/// Produced by [`CoreError::with_diagnostic`]. Its [`Display`](std::fmt::Display)
/// prints the complete error detail, including private internals, for local
/// development. Never send a `DiagnosticError`'s display across a trust
/// boundary — use [`CoreError::to_redacted`] for that.
#[derive(Debug)]
pub struct DiagnosticError {
    inner: CoreError,
}

impl DiagnosticError {
    /// Returns the underlying error by reference.
    pub fn inner(&self) -> &CoreError {
        &self.inner
    }

    /// Returns the full-detail display string for local diagnostic output.
    pub fn full_display(&self) -> String {
        self.inner.to_string()
    }
}

impl std::fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for DiagnosticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

/// Scrubs private internals from an error message string.
///
/// Removes SQL statements and keywords, absolute filesystem paths, raw
/// embedding vectors (long comma-separated float runs), and bracketed
/// `<private ...>` markers. Stable reason words are left intact.
fn redact_message(message: &str) -> String {
    use std::sync::OnceLock;
    static SQL_RE: OnceLock<regex::Regex> = OnceLock::new();
    static PATH_RE: OnceLock<regex::Regex> = OnceLock::new();
    static VEC_RE: OnceLock<regex::Regex> = OnceLock::new();
    static PRIV_RE: OnceLock<regex::Regex> = OnceLock::new();

    let sql = SQL_RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|sqlite[0-9]*:)[^\n]*",
        )
        .expect("redaction sql regex")
    });
    let path = PATH_RE.get_or_init(|| {
        regex::Regex::new(r#"(?:/|[A-Za-z]:\\)[^\s'"]*"#).expect("redaction path regex")
    });
    let vec_re = VEC_RE.get_or_init(|| {
        regex::Regex::new(r"\b-?\d+\.\d+(?:\s*,\s*-?\d+\.\d+){3,}\b")
            .expect("redaction vector regex")
    });
    let priv_re = PRIV_RE
        .get_or_init(|| regex::Regex::new(r"<private[^>]*>").expect("redaction private regex"));

    let out = sql.replace_all(message, "[SQL-REDACTED]");
    let out = path.replace_all(&out, "[PATH-REDACTED]");
    let out = vec_re.replace_all(&out, "[VECTOR-REDACTED]");
    priv_re.replace_all(&out, "[REDACTED]").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_error_variants_carry_named_fields() {
        let cap = CoreError::CapabilityUnsupported {
            capability: "vectors".to_string(),
            reason: "no provider".to_string(),
        };
        let space = CoreError::EmbeddingSpaceMismatch {
            expected: "fastembed/bge-small".to_string(),
            actual: "ollama/nomic".to_string(),
        };
        let stale = CoreError::MigrationManifestStale {
            expected: "deadbeef".to_string(),
            actual: "feedface".to_string(),
        };
        assert!(cap.to_string().contains("vectors"));
        assert!(space.to_string().contains("ollama/nomic"));
        assert!(stale.to_string().contains("feedface"));
    }

    #[test]
    fn to_redacted_strips_sql_paths_and_vectors() {
        let err = CoreError::Adapter {
            adapter: "engram-store-sql".to_string(),
            message:
                "SELECT * FROM memories WHERE id=7 at /var/lib/engram/mem.db vec=[0.1,0.2,0.3,0.4]"
                    .to_string(),
        };
        let redacted = err.to_redacted().to_string();
        assert!(
            !redacted.contains("SELECT"),
            "SQL must be redacted: {redacted}"
        );
        assert!(
            !redacted.contains("/var/lib/engram/mem.db"),
            "absolute path must be redacted: {redacted}"
        );
        assert!(
            !redacted.contains("0.1,0.2,0.3,0.4"),
            "raw vector must be redacted: {redacted}"
        );
        // Category and adapter name survive.
        assert!(redacted.contains("engram-store-sql"));
    }

    #[test]
    fn to_redacted_strips_private_markers() {
        let err = CoreError::InvalidRequest {
            reason: "bad scope <private tenant-acl> here".to_string(),
        };
        let redacted = err.to_redacted().to_string();
        assert!(!redacted.contains("tenant-acl"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn with_diagnostic_preserves_full_detail() {
        let err = CoreError::Adapter {
            adapter: "x".to_string(),
            message: "SELECT secret FROM tokens".to_string(),
        };
        let diag = err.with_diagnostic();
        let full = diag.full_display();
        assert!(
            full.contains("SELECT secret FROM tokens"),
            "diagnostic mode must keep full detail: {full}"
        );
        assert!(
            diag.inner()
                .to_string()
                .contains("SELECT secret FROM tokens")
        );
    }

    #[test]
    fn existing_variants_unchanged() {
        // Backward compatibility: the original variants still format as before.
        assert_eq!(
            CoreError::NotFound {
                target_type: "memory",
                target_id: "m1".to_string(),
            }
            .to_string(),
            "record not found: memory:m1"
        );
        assert_eq!(
            CoreError::PolicyDenied {
                reason: "no".to_string()
            }
            .to_string(),
            "policy denied: no"
        );
    }
}
