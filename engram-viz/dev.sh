#!/usr/bin/env bash
# engram-viz — start the backend (:3001) + frontend (:5173) together.
# Logs interleave on one stream; Ctrl-C stops both (kills the process group).
# Run from the repo root via:  pnpm viz
set -uo pipefail
cd "$(dirname "$0")/.." # repo root — pnpm-workspace.yaml lives here

echo "▸ engram-viz: starting backend (:3001, tsx watch) + frontend (:5173, vite)…"
echo "▸ open http://localhost:5173  (proxies /api → :3001)"
pnpm --filter engram-viz-backend run dev &
pnpm --filter engram-viz-frontend run dev &
trap 'kill 0' INT TERM EXIT
wait
