#!/usr/bin/env bash
# engram-mcp-http control (RFC-0017 Phase E): the HTTP / remote MCP surface.
#   pnpm mcp start  – launch engram-mcp-http detached on :8788 (loopback), then return
#   pnpm mcp stop   – stop it
#   pnpm mcp dev    – foreground, streamed logs (Ctrl-C stops)
#   pnpm mcp logs   – tail the log
#   pnpm mcp        – same as `start`
#
# Binds 127.0.0.1 (loopback) by default — no auth token needed. To expose on
# the network instead, set MCP_HOST=0.0.0.0 + MCP_AUTH_TOKEN=<secret> (bin.ts
# requires a token for non-loopback), and MCP_TLS_CERT / MCP_TLS_KEY for HTTPS.
set -uo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"   # repo root — pnpm-workspace.yaml lives here
BIN="$REPO/packages/runtime/dist/mcp/bin.js"

# Source the local env file if present (LLM / store / MCP config). `set -a`
# exports the vars so the spawned engram-mcp-http (and the engram-maintain child
# it spawns for maintenance) inherit them.
if [ -f "$REPO/.env" ]; then set -a; . "$REPO/.env"; set +a; fi

# Ensure the store directory exists — the provider validates trusted_root at open.
mkdir -p "${ENGRAM_STORAGE:-~/.engram/agentzero}"

# Portable detach: setsid (Linux) creates a new session; macOS uses plain background.
DETACH=""
command -v setsid >/dev/null 2>&1 && DETACH="setsid"

MCP_PORT="${MCP_PORT:-8788}"
MCP_HOST="${MCP_HOST:-127.0.0.1}"
ENGRAM_STORAGE="${ENGRAM_STORAGE:-$HOME/.engram/agentzero}"
ENGRAM_DB_FILE="${ENGRAM_DB_FILE:-engram_data.db}"

PIDFILE="/tmp/engram-mcp.pids"
LOG="/tmp/engram-mcp.log"
CFGFILE="/tmp/engram-mcp-config.json"

alive() { [ -n "${1:-}" ] && kill -0 "$1" 2>/dev/null; }
port_taken() { lsof -ti :"$1" >/dev/null 2>&1; }

# Writes the v1 EngramConfig JSON the binding decodes. Same shape as the viz
# BFF (engram-cc/backend/src/engram/provider.ts::buildConfigJson): single-file
# store, DryRun migration (no schema writes on boot — the store already exists),
# vectors off (the agentzero store reports retrieval/vectors unsupported, and
# skipping the FastEmbed load keeps boot fast). Regenerated each start so env
# changes (ENGRAM_STORAGE / ENGRAM_DB_FILE) take effect without a committed file.
write_config() {
  cat >"$CFGFILE" <<JSON
{
  "storage_path": "$ENGRAM_STORAGE",
  "trusted_root": "$ENGRAM_STORAGE",
  "scope_policy": "Strict",
  "embedding_provider": { "provider_type": "none", "model": "none", "dimensions": 384, "prompt_profile": "query" },
  "migration_mode": "DryRun",
  "capability_policy": "FailClosed",
  "sqlite_storage_layout": { "kind": "single_file", "file_name": "$ENGRAM_DB_FILE" },
  "enable_vector": false
}
JSON
}

# Optional flags for the network case. Loopback default → none of these set.
extra_args() {
  local out=""
  [ -n "${MCP_AUTH_TOKEN:-}" ] && out="$out --auth-token $MCP_AUTH_TOKEN"
  [ -n "${MCP_TLS_CERT:-}" ] && [ -n "${MCP_TLS_KEY:-}" ] && out="$out --tls-cert $MCP_TLS_CERT --tls-key $MCP_TLS_KEY"
  [ -n "${MCP_ONTOLOGY:-}" ] && out="$out --ontology $MCP_ONTOLOGY"
  [ -n "${MCP_TAXONOMY:-}" ] && out="$out --taxonomy $MCP_TAXONOMY"
  printf '%s' "$out"
}

start() {
  if [ -f "$PIDFILE" ]; then
    # shellcheck disable=SC1090
    . "$PIDFILE" 2>/dev/null
    if alive "${MCP_PID:-}" || port_taken "$MCP_PORT"; then
      echo "engram-mcp-http already running — use 'pnpm mcp stop' first."; return 0
    fi
  fi
  [ -f "$BIN" ] || { echo "✗ $BIN missing — run: pnpm --filter @engram/runtime run build" >&2; exit 1; }
  write_config
  echo "▸ starting engram-mcp-http on http://$MCP_HOST:$MCP_PORT/mcp (storage $ENGRAM_STORAGE)…"
  # setsid detaches into its own session so it survives this script exiting;
  # the stored PID is the session leader (== process-group id) for clean shutdown.
  $DETACH node "$BIN" --config "$CFGFILE" --port "$MCP_PORT" --host "$MCP_HOST" $(extra_args) >"$LOG" 2>&1 </dev/null &
  MCP_PID=$!
  printf 'MCP_PID=%s\n' "$MCP_PID" >"$PIDFILE"
  for _ in $(seq 1 30); do port_taken "$MCP_PORT" && break; sleep 1; done
  if port_taken "$MCP_PORT"; then
    echo "✓ engram-mcp-http up — http://$MCP_HOST:$MCP_PORT/mcp  (logs: $LOG)"
  else
    echo "⚠ did not bind in time — check $LOG"; tail -n 20 "$LOG" >&2
  fi
}

stop() {
  if [ -f "$PIDFILE" ]; then
    # shellcheck disable=SC1090
    . "$PIDFILE" 2>/dev/null
    alive "${MCP_PID:-}" && kill "${MCP_PID}" 2>/dev/null
    rm -f "$PIDFILE"
  fi
  # belt-and-suspenders: free the port regardless
  ids=$(lsof -ti :"$MCP_PORT" 2>/dev/null); [ -n "$ids" ] && kill $ids 2>/dev/null
  echo "✓ engram-mcp-http stopped"
}

dev() {
  [ -f "$BIN" ] || { echo "✗ $BIN missing — run: pnpm --filter @engram/runtime run build" >&2; exit 1; }
  write_config
  echo "▸ engram-mcp-http foreground on http://$MCP_HOST:$MCP_PORT/mcp; Ctrl-C stops"
  node "$BIN" --config "$CFGFILE" --port "$MCP_PORT" --host "$MCP_HOST" $(extra_args)
}

logs() { tail -f "$LOG"; }

case "${1:-start}" in
  start) start ;;
  stop) stop ;;
  dev) dev ;;
  logs) logs ;;
  *) echo "usage: pnpm mcp [start|stop|dev|logs]" >&2; exit 2 ;;
esac
