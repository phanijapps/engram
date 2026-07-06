//! Redaction utilities for sensitive configuration paths.
//!
//! This module provides helpers to redact sensitive information from
//! configuration and error messages while preserving structure for debugging.

use std::collections::HashSet;

/// Default sensitive key patterns that should be redacted.
///
/// These patterns match common configuration keys that contain sensitive
/// information like passwords, tokens, API keys, and secrets.
pub fn default_sensitive_keys() -> HashSet<&'static str> {
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "accesskey",
        "private_key",
        "privatekey",
        "auth_token",
        "authtoken",
        "bearer_token",
        "bearertoken",
        "refresh_token",
        "refreshtoken",
        "session_token",
        "sessiontoken",
        "csrf_token",
        "csrftoken",
        "api_secret",
        "apisecret",
        "webhook_secret",
        "webhooksecret",
        "client_secret",
        "clientsecret",
        "signing_key",
        "signingkey",
    ]
    .iter()
    .cloned()
    .collect()
}

/// Redacts sensitive values from a key-value string representation.
///
/// This function checks if the key matches any sensitive pattern and
/// replaces the value with `[REDACTED]` if it does. Keys are compared
/// case-insensitively.
///
/// # Arguments
///
/// * `key` - The configuration key name
/// * `value` - The configuration value (may be sensitive)
///
/// # Returns
///
/// * The original value if the key is not sensitive, `[REDACTED]` otherwise
///
/// # Example
///
/// ```
/// use engram_runtime::redaction::maybe_redact_value;
///
/// assert_eq!(maybe_redact_value("api_key", "secret123"), "[REDACTED]");
/// assert_eq!(maybe_redact_value("timeout", "30"), "30");
/// ```
pub fn maybe_redact_value(key: &str, value: &str) -> String {
    let sensitive_keys = default_sensitive_keys();
    let key_lower = key.to_lowercase();

    // Check if any sensitive pattern appears in the key
    let is_sensitive = sensitive_keys
        .iter()
        .any(|sensitive| key_lower.contains(sensitive) || key_lower.ends_with(sensitive));

    if is_sensitive && !value.is_empty() {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

/// Redacts sensitive information from a path string.
///
/// This function checks if path components contain sensitive patterns and
/// redacts them while preserving the path structure.
///
/// # Arguments
///
/// * `path` - The file path or URL that may contain sensitive information
///
/// # Returns
///
/// * The path with sensitive components redacted
///
/// # Example
///
/// ```
/// use engram_runtime::redaction::redact_path;
///
/// let path = "https://user:password@api.example.com/endpoint";
/// let redacted = redact_path(path);
/// assert!(redacted.contains("[REDACTED]"));
/// ```
pub fn redact_path(path: &str) -> String {
    let mut result = path.to_string();

    // Redact URL credentials if present
    if let Some(start) = result.find("://") {
        let after_protocol = &result[start + 3..];
        if let Some(at_pos) = after_protocol.find('@') {
            let credential_part = &after_protocol[..at_pos];
            if credential_part.contains(':') {
                // Redact password portion of credentials
                let parts: Vec<&str> = credential_part.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let redacted_creds = format!("{}:[REDACTED]", parts[0]);
                    result = result.replace(
                        &format!("{}{}@", &result[..start + 3], credential_part),
                        &format!("{}{}@", &result[..start + 3], &redacted_creds),
                    );
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_sensitive_keys_contains_common_patterns() {
        let keys = default_sensitive_keys();
        assert!(keys.contains("password"));
        assert!(keys.contains("token"));
        assert!(keys.contains("api_key"));
        assert!(keys.contains("secret"));
    }

    #[test]
    fn test_maybe_redact_value_redacts_sensitive_keys() {
        assert_eq!(maybe_redact_value("api_key", "secret123"), "[REDACTED]");
        assert_eq!(maybe_redact_value("API_KEY", "secret123"), "[REDACTED]");
        assert_eq!(maybe_redact_value("my_password", "secret123"), "[REDACTED]");
        assert_eq!(maybe_redact_value("auth_token", "secret123"), "[REDACTED]");
    }

    #[test]
    fn test_maybe_redact_value_preserves_non_sensitive_keys() {
        assert_eq!(maybe_redact_value("timeout", "30"), "30");
        assert_eq!(maybe_redact_value("max_retries", "3"), "3");
        assert_eq!(maybe_redact_value("host", "localhost"), "localhost");
    }

    #[test]
    fn test_maybe_redact_value_handles_empty_values() {
        assert_eq!(maybe_redact_value("password", ""), "");
    }

    #[test]
    fn test_redact_path_redacts_url_credentials() {
        let path = "https://user:password@api.example.com/endpoint";
        let redacted = redact_path(path);
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("user"));
        assert!(!redacted.contains("password"));
    }

    #[test]
    fn test_redact_path_preserves_urls_without_credentials() {
        let path = "https://api.example.com/endpoint";
        let redacted = redact_path(path);
        assert_eq!(redacted, path);
    }

    #[test]
    fn test_redact_path_preserves_file_paths() {
        let path = "/var/lib/engram/data.db";
        let redacted = redact_path(path);
        assert_eq!(redacted, path);
    }
}
