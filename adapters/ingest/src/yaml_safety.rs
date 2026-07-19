//! YAML safety checking for untrusted repository content.
//!
//! Provides conservative limits for YAML parsing to prevent billion-laugh attacks,
//! fat-base expansion, flow-nested stack overflow, and single-line bomb attacks.
//! These bounds protect against malicious YAML files discovered during repository scanning.

/// Conservative limits for untrusted YAML from arbitrary repositories.
///
/// Anchor/alias caps are single-digit: legitimate OpenAPI specs essentially
/// never use YAML anchors (they use JSON Pointer `$ref` instead). A low cap
/// bounds the amplification factor even for fat-base attacks where one large
/// anchor is referenced many times.
///
/// The flow-collection depth cap guards against stack overflows driven by
/// deeply nested flow-style `[[[[...]]]` on a single line — a vector that
/// bypasses leading-indent measurement entirely.
pub const YAML_MAX_ANCHORS: usize = 4; // anchor definitions (&name)
pub const YAML_MAX_ALIASES: usize = 4; // alias references (*name)
pub const YAML_MAX_INDENT: usize = 128; // leading-whitespace chars → ~64 block levels
pub const YAML_MAX_FLOW_DEPTH: usize = 128; // running [{…}] depth across whole doc
pub const YAML_MAX_LINE_BYTES: usize = 64 * 1024; // per-line cap: closes the whole single-line-bomb family

/// Cheap pre-scan that rejects YAML inputs with pathological anchor/alias
/// density, excessive block-nesting depth, or excessive flow-collection depth
/// before handing control to the full serde_yml parser.
///
/// Vectors closed by this guard:
/// - Billion-laughs (deeply nested aliases): caught by alias count cap.
/// - Fat-base expansion (one large anchor × many aliases): caught by alias cap.
/// - Flow-nested stack overflow (`[[[[...]]]` on one line, indent = 0): caught
///   by the running `[`/`{` depth counter.
/// - Compact block-sequence chains (`- - - - x` on one line = nested block seqs
///   with zero leading indent and no `[`/`{`): caught by the per-line byte cap,
///   which closes the entire single-line-bomb family in one check.
///
/// Returns `Err(reason)` on a suspicious input; `Ok(())` when the text looks
/// safe to parse. False positives (legitimate spec with many `*`/`[`/`{` in
/// values) are acceptable — the file is skipped with a warning, not crashed.
pub fn check_yaml_safety(text: &str) -> Result<(), String> {
    let mut anchors: usize = 0;
    let mut aliases: usize = 0;
    // Running flow-collection depth tracked across all lines so single-line
    // deeply-nested flow docs are caught regardless of leading indent.
    let mut flow_depth: usize = 0;
    for (i, line) in text.lines().enumerate() {
        // Per-line byte cap: a single physical line longer than this can only be
        // a pathological single-line bomb (`[[[…`, `{{{…`, or compact `- - - …`
        // block sequences that recurse in serde_yml with zero leading indent).
        // Genuine multi-line block nesting stays bounded by YAML_MAX_INDENT.
        if line.len() > YAML_MAX_LINE_BYTES {
            return Err(format!(
                "YAML line {} length ({} bytes) exceeds safety limit ({YAML_MAX_LINE_BYTES})",
                i + 1,
                line.len()
            ));
        }
        // Compact block-sequence chains (`- - - … x`) nest one level per `- `
        // block-entry on a single physical line and carry no closing token to
        // track via flow_depth. Bound the per-line block-entry count to the same
        // depth cap, so all single-line nesting forms (`[`, `{`, `- `) are limited
        // to YAML_MAX_FLOW_DEPTH — keeping serde_yml's recursion well within a
        // rayon worker stack even for lines under the byte cap.
        let dash_entries = line.matches("- ").count();
        if dash_entries > YAML_MAX_FLOW_DEPTH {
            return Err(format!(
                "YAML compact block-sequence depth ({dash_entries}) exceeds safety limit \
                 ({YAML_MAX_FLOW_DEPTH}) at line {}",
                i + 1
            ));
        }
        // Count YAML anchor/alias sigils — rough proxy that covers billion-laughs
        // and fat-base alias patterns.
        anchors += line.chars().filter(|&c| c == '&').count();
        aliases += line.chars().filter(|&c| c == '*').count();
        if anchors > YAML_MAX_ANCHORS {
            return Err(format!(
                "YAML anchor count ({anchors}) exceeds safety limit ({YAML_MAX_ANCHORS}) \
                 at line {}",
                i + 1
            ));
        }
        if aliases > YAML_MAX_ALIASES {
            return Err(format!(
                "YAML alias count ({aliases}) exceeds safety limit ({YAML_MAX_ALIASES}) \
                 at line {}",
                i + 1
            ));
        }
        // Track flow-collection depth (running counter: `[`/`{` open, `]`/`}` close).
        // This catches `x: [[[[...]]]` bombs that have zero leading indent.
        for c in line.chars() {
            match c {
                '[' | '{' => {
                    flow_depth += 1;
                    if flow_depth > YAML_MAX_FLOW_DEPTH {
                        return Err(format!(
                            "YAML flow-collection depth ({flow_depth}) exceeds safety \
                             limit ({YAML_MAX_FLOW_DEPTH}) at line {}",
                            i + 1
                        ));
                    }
                }
                ']' | '}' => {
                    flow_depth = flow_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        // Block-nesting depth from leading whitespace.
        let indent = line.len() - line.trim_start().len();
        if indent > YAML_MAX_INDENT {
            return Err(format!(
                "YAML block-nesting depth (indent {indent}) exceeds safety limit \
                 ({YAML_MAX_INDENT} chars) at line {}",
                i + 1
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real billion-laughs bomb: the alias references (`*a`) far exceed the
    /// single-digit alias budget, so the pre-scan rejects it before serde_yml
    /// can expand it.
    #[test]
    fn yaml_bomb_is_rejected() {
        let bomb = "a: &a [\"x\",\"x\"]\n\
                    b: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a,*a]\n\
                    c: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b,*b]\n";
        assert!(
            check_yaml_safety(bomb).is_err(),
            "billion-laughs alias bomb must be rejected"
        );
    }

    /// Deeply nested flow collections on one line exceed the flow-depth cap
    /// (`[` count > `YAML_MAX_FLOW_DEPTH`), regardless of zero leading indent.
    #[test]
    fn flow_nested_yaml_is_rejected() {
        let malicious = format!("x: {}", "[".repeat(YAML_MAX_FLOW_DEPTH + 1));
        assert!(
            check_yaml_safety(&malicious).is_err(),
            "flow-nested depth beyond the cap must be rejected"
        );
    }

    /// Compact nested block sequences (`- - - … x`) recurse one level per `- `
    /// with no leading indent and no flow tokens; the per-line block-entry cap
    /// rejects them when the count exceeds `YAML_MAX_FLOW_DEPTH`.
    #[test]
    fn compact_block_seq_yaml_is_rejected() {
        let malicious = format!("- {}", "- ".repeat(YAML_MAX_FLOW_DEPTH + 1));
        assert!(
            check_yaml_safety(&malicious).is_err(),
            "compact block-sequence depth beyond the cap must be rejected"
        );
    }

    /// One large anchored base referenced by many aliases (fat-base
    /// expansion): the alias count exceeds the budget.
    #[test]
    fn fat_base_yaml_is_rejected() {
        let malicious = "anchor: &huge\nalias1: *anchor\nalias2: *anchor\nalias3: *anchor\nalias4: *anchor\nalias5: *anchor";
        assert!(check_yaml_safety(malicious).is_err());
    }

    /// Anchor count above the single-digit budget is rejected.
    #[test]
    fn excess_anchors_are_rejected() {
        let malicious = "&a x\n&b y\n&c z\n&d w\n&e v\n&f u";
        assert!(check_yaml_safety(malicious).is_err());
    }

    /// Leading-whitespace indent above the cap signals pathological block depth.
    #[test]
    fn excess_indent_is_rejected() {
        let malicious = format!("{}value", " ".repeat(YAML_MAX_INDENT + 1));
        assert!(check_yaml_safety(&malicious).is_err());
    }

    /// A single physical line beyond the byte cap closes the single-line-bomb
    /// family (minified pathological payloads).
    #[test]
    fn excess_line_bytes_are_rejected() {
        let malicious = format!("x: {}", "a".repeat(YAML_MAX_LINE_BYTES + 1));
        assert!(check_yaml_safety(&malicious).is_err());
    }

    /// Modest nesting well under every cap is legitimate and must pass — the
    /// guard rejects pathological depth, not ordinary structure.
    #[test]
    fn modest_nesting_is_accepted() {
        let doc = "x:\n  - - - - - - - - - - - - x\ny: [[[[[[[[[[[[[[[]]]]]]]]]]]]]]";
        assert!(
            check_yaml_safety(doc).is_ok(),
            "modest nesting under the caps must not be a false positive"
        );
    }

    #[test]
    fn legitimate_openapi_passes() {
        let spec = r#"
openapi: 3.0.0
info:
  title: Sample API
paths:
  /users:
    get:
      summary: Get users
      responses:
        "200":
          content:
            application/json: {}
"#;
        assert!(check_yaml_safety(spec).is_ok());
    }
}
