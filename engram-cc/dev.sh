#!/usr/bin/env bash
# engram-cc dev-server control — starts ALL THREE: backend + MCP + frontend.
#   pnpm cc start   – launch backend (:3001) + MCP (:8788) + frontend (:5173)
#   pnpm cc stop    – stop all three
#   pnpm cc dev     – foreground, streamed logs (Ctrl-C stops all)
#   pnpm cc logs    – tail all three server logs
#   pnpm cc         – same as `start`
set -uo pipefail
cd "$(dirname "$0")/.." # repo root — pnpm-workspace.yaml lives here

# Source the local env file if present (LLM / store / BFF / MCP config). `set -a`
# exports the vars so the spawned backend + frontend (and the engram-maintain
# child the BFF spawns for maintenance) all inherit them.
if [ -f .env ]; then set -a; . ./.env; set +a; fi

BE_PORT=3001
FE_PORT=5173
MCP_DEV="$PWD/mcp/dev.sh"
PIDFILE="/tmp/engram-cc.pids"
BE_LOG="/tmp/engram-cc-backend.log"
FE_LOG="/tmp/engram-cc-frontend.log"
MCP_LOG="/tmp/engram-mcp.log"

alive() { [ -n "${1:-}" ] && kill -0 "$1" 2>/dev/null; }
port_taken() { lsof -ti :"$1" >/dev/null 2>&1; }

start() {
  # already running?
  if [ -f "$PIDFILE" ]; then
    # shellcheck disable=SC1090
    . "$PIDFILE" 2>/dev/null
    if alive "${BE_PID:-}" || alive "${FE_PID:-}" || port_taken "$BE_PORT" || port_taken "$FE_PORT"; then
      echo "engram-cc already running — use 'pnpm cc stop' first."; return 0
    fi
  fi
  echo "▸ starting backend (:$BE_PORT) + MCP (:8788) + frontend (:$FE_PORT)…"
  # setsid detaches each into its own session so they survive this script exiting;
  # the stored PID is the session leader (== process-group id) for clean shutdown.
  setsid pnpm --filter engram-cc-backend run dev >"$BE_LOG" 2>&1 </dev/null &
  BE_PID=$!
  setsid pnpm --filter engram-cc-frontend run dev >"$FE_LOG" 2>&1 </dev/null &
  FE_PID=$!
  printf 'BE_PID=%s\nFE_PID=%s\n' "$BE_PID" "$FE_PID" >"$PIDFILE"
  # wait for both ports to bind (≤ ~30s)
  for _ in $(seq 1 30); do
    port_taken "$BE_PORT" && port_taken "$FE_PORT" && break
    sleep 1
  done
  if port_taken "$BE_PORT" && port_taken "$FE_PORT"; then
    echo "✓ backend + frontend up — http://localhost:$FE_PORT"
  else
    echo "⚠ a server did not bind in time — check $BE_LOG / $FE_LOG"
  fi
  # Start the MCP server (:8788) — reuses mcp/dev.sh (own PID file + config).
  bash "$MCP_DEV" start
}

stop() {
  if [ -f "$PIDFILE" ]; then
    # shellcheck disable=SC1090
    . "$PIDFILE" 2>/dev/null
    for pid in "${BE_PID:-}" "${FE_PID:-}"; do
      alive "$pid" && kill -- -"$pid" 2>/dev/null # kill the process group
    done
    rm -f "$PIDFILE"
  fi
  # belt-and-suspenders: free the ports regardless
  for p in "$BE_PORT" "$FE_PORT"; do
    ids=$(lsof -ti :"$p" 2>/dev/null); [ -n "$ids" ] && kill $ids 2>/dev/null
  done
  # Stop the MCP server.
  bash "$MCP_DEV" stop
  echo "✓ engram-cc stopped (backend + MCP + frontend)"
}

dev() {
  echo "▸ engram-cc foreground — backend (:$BE_PORT) + MCP (:8788) + frontend (:$FE_PORT); Ctrl-C stops all"
  pnpm --filter engram-cc-backend run dev &
  pnpm --filter engram-cc-frontend run dev &
  bash "$MCP_DEV" dev &
  trap 'kill 0' INT TERM EXIT
  wait
}

logs() { tail -f "$BE_LOG" "$FE_LOG" "$MCP_LOG"; }

case "${1:-start}" in
  start) start ;;
  stop) stop ;;
  dev) dev ;;
  logs) logs ;;
  *) echo "usage: pnpm cc [start|stop|dev|logs]" >&2; exit 2 ;;
esac
