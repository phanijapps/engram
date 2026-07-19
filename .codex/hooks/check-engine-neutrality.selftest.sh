#!/usr/bin/env bash
# check-engine-neutrality.selftest.sh — TDD parametric injection test for the
# ADR-0022 rule-1 gate. Verifies the gate is green on the real gated surface AND
# fires on each of the three forbidden pattern classes, without mutating real
# source (it drives the gate via path args on throwaway temp files).
#
#   cargo test cannot run a shell gate; run directly:
#     bash .codex/hooks/check-engine-neutrality.selftest.sh
set -u

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
GATE="$ROOT/.codex/hooks/check-engine-neutrality.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail=0
expect_zero() {     # label, then command...
  local label="$1"; shift
  if "$@" >/tmp/neu-out 2>&1; then rc=0; else rc=$?; fi
  if [ "$rc" -eq 0 ]; then echo "PASS: $label (clean)"; else
    echo "FAIL: $label should be clean (rc=$rc)"; cat /tmp/neu-out; fail=$((fail+1)); fi
}
expect_nonzero() {  # label, then command...
  local label="$1"; shift
  if "$@" >/tmp/neu-out 2>&1; then rc=0; else rc=$?; fi
  if [ "$rc" -ne 0 ]; then echo "PASS: $label (fired)"; else
    echo "FAIL: $label should fire but gate was clean"; cat /tmp/neu-out; fail=$((fail+1)); fi
}

# Baseline: the real gated surface is clean today.
expect_zero "real gated surface" bash "$GATE"

# A clean Rust file in a temp path is clean.
cat > "$TMP/clean.rs" <<'EOF'
pub fn add(a: i32, b: i32) -> i32 { a + b }
EOF
expect_zero "clean temp file" bash "$GATE" "$TMP/clean.rs"

# Class (a): a Sql* engine type name → must fire.
cat > "$TMP/a_type.rs" <<'EOF'
pub struct Holder { store: SqlKnowledgeStore }
EOF
expect_nonzero "class (a) Sql* type name" bash "$GATE" "$TMP/a_type.rs"

# Class (b): an engine-crate import → must fire (top-level and indented + path-ref).
printf 'use rusqlite::Connection;\n' > "$TMP/b_import.rs"
expect_nonzero "class (b) top-level use" bash "$GATE" "$TMP/b_import.rs"

cat > "$TMP/b_indented.rs" <<'EOF'
fn f() {
    use rusqlite::Connection;
    let c = rusqlite::Connection::open_in_memory();
}
EOF
expect_nonzero "class (b) indented use + path-ref" bash "$GATE" "$TMP/b_indented.rs"

# Class (c): raw-SQL string literals → must fire (query + PRAGMA).
cat > "$TMP/c_sql.rs" <<'EOF'
const Q: &str = "SELECT id FROM memories WHERE x = 1";
EOF
expect_nonzero "class (c) SELECT literal" bash "$GATE" "$TMP/c_sql.rs"

cat > "$TMP/c_pragma.rs" <<'EOF'
const P: &str = "PRAGMA journal_mode=WAL";
EOF
expect_nonzero "class (c) PRAGMA literal" bash "$GATE" "$TMP/c_pragma.rs"

echo "---"
if [ "$fail" -eq 0 ]; then echo "ALL PASS"; exit 0; else echo "$fail FAILURE(S)"; exit 1; fi
