#!/usr/bin/env bash
# Install the codegraph MCP server for Codex CLI and/or Claude Code.
# Usage: ./install.sh [--codex] [--claude] [--cursor]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/../../target/release/engram-codegraph-mcp"

if [ ! -f "$BINARY" ]; then
    echo "Building engram-codegraph-mcp (release)..."
    (cd "$SCRIPT_DIR/../.." && cargo build -p engram-codegraph-mcp --release)
fi

INSTALL_CODEX=false
INSTALL_CLAUDE=false
INSTALL_CURSOR=false

for arg in "$@"; do
    case "$arg" in
        --codex)  INSTALL_CODEX=true ;;
        --claude) INSTALL_CLAUDE=true ;;
        --cursor) INSTALL_CURSOR=true ;;
        --all)    INSTALL_CODEX=true; INSTALL_CLAUDE=true; INSTALL_CURSOR=true ;;
    esac
done

# Default: install for all if no flags
if [ "$INSTALL_CODEX" = false ] && [ "$INSTALL_CLAUDE" = false ] && [ "$INSTALL_CURSOR" = false ]; then
    INSTALL_CODEX=true; INSTALL_CLAUDE=true
fi

# --- Codex CLI ---
if [ "$INSTALL_CODEX" = true ]; then
    CODEX_CONFIG="$HOME/.codex/config.toml"
    mkdir -p "$(dirname "$CODEX_CONFIG")"
    if grep -q "\[mcp_servers.codegraph\]" "$CODEX_CONFIG" 2>/dev/null; then
        echo "[codex] codegraph MCP already configured in $CODEX_CONFIG"
    else
        cat >> "$CODEX_CONFIG" << TOML

[mcp_servers.codegraph]
command = "$BINARY"
args = []
TOML
        echo "[codex] added codegraph MCP server to $CODEX_CONFIG"
    fi
fi

# --- Claude Code ---
if [ "$INSTALL_CLAUDE" = true ]; then
    CLAUDE_CONFIG="$HOME/.claude/settings.json"
    echo "[claude] add this to your project .mcp.json or ~/.claude/settings.json:"
    cat << JSON
{
  "mcpServers": {
    "codegraph": {
      "command": "$BINARY",
      "args": []
    }
  }
}
JSON
fi

# --- Cursor ---
if [ "$INSTALL_CURSOR" = true ]; then
    echo "[cursor] add this to your ~/.cursor/mcp.json:"
    cat << JSON
{
  "mcpServers": {
    "codegraph": {
      "command": "$BINARY",
      "args": []
    }
  }
}
JSON
fi

# --- Skills ---
SKILL_DIR="$SCRIPT_DIR/../skills"
for agent_dir in .codex/skills .claude/skills; do
    AGENT_SKILLS="$SCRIPT_DIR/../../$agent_dir"
    if [ -d "$AGENT_SKILLS" ]; then
        for skill in codegraph-first codegraph-impact codegraph-dead-code codegraph-onboarding; do
            mkdir -p "$AGENT_SKILLS/$skill"
            cp "$SKILL_DIR/$skill/SKILL.md" "$AGENT_SKILLS/$skill/SKILL.md" 2>/dev/null || true
        done
        echo "[skills] installed 4 codegraph skills to $agent_dir"
    fi
done

echo ""
echo "Done. The codegraph MCP server exposes 14 tools."
echo "Binary: $BINARY"
