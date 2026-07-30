#!/usr/bin/env bash
# check-surface-parity.sh — ADR-0022 surface-parity gate.
#
# Asserts every provider `require_*` capability on the Rust facade
# (core/integration/src/provider.rs) is reachable from the N-API binding
# (bindings/node/src/provider.rs): either via a `require*Api` proxy on
# NativeProvider, via a documented special-case surface, or listed as
# acknowledged debt. Mirrors the role check-engine-neutrality.sh plays for
# rule-1 (engine neutrality).
#
# Why: ADR-0022 — a capability is not "shipped" until BOTH a Rust embedder
# (engram-integration) and a TS/N-API agent (bindings/node) can invoke it.
# This lint makes drift fail the gate: add a require_* to the facade without a
# matching proxy (or an explicit debt entry) and CI fails.
#
# Usage:
#   check-surface-parity.sh            # scan the default facade + binding
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
FACADE="$ROOT/core/integration/src/provider.rs"
BINDING="$ROOT/bindings/node/src/provider.rs"

# Capabilities reached via a DIFFERENT binding surface (not a require*Api proxy).
# Each entry: "<facade require_X name>=><how it is reached>"
SPECIAL_CASES=(
  "knowledge=>requireGraphApi (NativeGraphApi holds knowledge + graph)"
  "consolidation=>consolidateJson (direct method on NativeProvider)"
)

# Acknowledged debt: provider handles with NO N-API proxy yet. These are
# low-level composition/embedding seams; shrink this list by adding proxies,
# then removing the entry here.
DEBT_ALLOWLIST=()

[ -f "$FACADE" ] || { echo "facade not found: $FACADE"; exit 2; }
[ -f "$BINDING" ] || { echo "binding not found: $BINDING"; exit 2; }

mapfile -t FACADE_REQUIRES < <(grep -oE 'pub fn require_[a-z_]+' "$FACADE" | sed 's/pub fn //' | sort -u)
mapfile -t BINDING_PROXIES < <(grep -oE 'require[A-Za-z]+Api' "$BINDING" | sort -u)

to_camel() {
  awk -F_ '{for(i=1;i<=NF;i++)printf "%s%s", toupper(substr($i,1,1)), substr($i,2); print ""}' <<<"$1"
}

missing=()
for req in "${FACADE_REQUIRES[@]}"; do
  cap="${req#require_}"
  if printf '%s\n' "${SPECIAL_CASES[@]}" | grep -q "^${cap}=>"; then continue; fi
  if printf '%s\n' "${DEBT_ALLOWLIST[@]}" | grep -qx "$cap"; then continue; fi
  proxy="require$(to_camel "$cap")Api"
  if printf '%s\n' "${BINDING_PROXIES[@]}" | grep -qx "$proxy"; then continue; fi
  missing+=("$cap (expected $proxy)")
done

if [ "${#missing[@]}" -gt 0 ]; then
  echo "ADR-0022 surface-parity violation — require_* on the facade with no N-API proxy:"
  printf '  - %s\n' "${missing[@]}"
  echo ""
  echo "Fix: add a require*Api proxy in bindings/node/src/provider.rs,"
  echo "     or record the capability in SPECIAL_CASES / DEBT_ALLOWLIST here."
  exit 1
fi

echo "surface parity check passed (${#FACADE_REQUIRES[@]} capabilities: proxied + ${#SPECIAL_CASES[@]} special + ${#DEBT_ALLOWLIST[@]} acknowledged-debt)"
