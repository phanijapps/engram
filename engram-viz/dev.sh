#!/usr/bin/env bash
# engram-viz dev-server control.
#   pnpm viz start   – launch backend (:3001) + frontend (:5173) detached, then return
#   pnpm viz stop    – stop both
#   pnpm viz dev     – foreground, streamed logs (Ctrl-C stops both)
#   pnpm viz logs    – tail both server logs
#   pnpm viz         – same as `start`
set -uo pipefail
cd "$(dirname "$0")/.." # repo root — pnpm-workspace.yaml lives here

BE_PORT=3001
FE_PORT=5173
PIDFILE="/tmp/engram-viz.pids"
BE_LOG="/tmp/engram-viz-backend.log"
FE_LOG="/tmp/engram-viz-frontend.log"

alive() { [ -n "${1:-}" ] && kill -0 "$1" 2>/dev/null; }
port_taken() { lsof -ti :"$1" >/dev/null 2>&1; }

start() {
  # already running?
  if [ -f "$PIDFILE" ]; then
    # shellcheck disable=SC1090
    . "$PIDFILE" 2>/dev/null
    if alive "${BE_PID:-}" || alive "${FE_PID:-}" || port_taken "$BE_PORT" || port_taken "$FE_PORT"; then
      echo "engram-viz already running — use 'pnpm viz stop' first."; return 0
    fi
  fi
  echo "▸ starting backend (:$BE_PORT, tsx watch) + frontend (:$FE_PORT, vite)…"
  # setsid detaches each into its own session so they survive this script exiting;
  # the stored PID is the session leader (== process-group id) for clean shutdown.
  setsid pnpm --filter engram-viz-backend run dev >"$BE_LOG" 2>&1 </dev/null &
  BE_PID=$!
  setsid pnpm --filter engram-viz-frontend run dev >"$FE_LOG" 2>&1 </dev/null &
  FE_PID=$!
  printf 'BE_PID=%s\nFE_PID=%s\n' "$BE_PID" "$FE_PID" >"$PIDFILE"
  # wait for both ports to bind (≤ ~30s)
  for _ in $(seq 1 30); do
    port_taken "$BE_PORT" && port_taken "$FE_PORT" && break
    sleep 1
  done
  if port_taken "$BE_PORT" && port_taken "$FE_PORT"; then
    echo "✓ engram-viz up — http://localhost:$FE_PORT  (logs: $BE_LOG, $FE_LOG)"
  else
    echo "⚠ a server did not bind in time — check $BE_LOG / $FE_LOG"
  fi
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
  echo "✓ engram-viz stopped"
}

dev() {
  echo "▸ engram-viz foreground — backend (:$BE_PORT) + frontend (:$FE_PORT); Ctrl-C stops both"
  pnpm --filter engram-viz-backend run dev &
  pnpm --filter engram-viz-frontend run dev &
  trap 'kill 0' INT TERM EXIT
  wait
}

logs() { tail -f "$BE_LOG" "$FE_LOG"; }

case "${1:-start}" in
  start) start ;;
  stop) stop ;;
  dev) dev ;;
  logs) logs ;;
  *) echo "usage: pnpm viz [start|stop|dev|logs]" >&2; exit 2 ;;
esac
