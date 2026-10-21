"use strict";
/**
 * `engram-mcp install --client <id>` (RFC-0021 Phase 2): register this
 * server with an MCP host by writing its config — one command, no
 * hand-edited JSON.
 *
 * Config entries default to the portable npx form so the registration
 * works on any machine with Node, independent of where the launcher
 * itself was installed from.
 *
 * Formats are per-host and verified against each host's docs during
 * Phase 2 implementation; any drift is a bug against this table.
 */

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const STORAGE_DEFAULT = path.join(os.homedir(), ".engram");

/** The server entry every host gets, in the host's own file. */
function serverArgs(storage, project, modeArgs) {
  return [
    "-y",
    `@engramlabs/mcp@${require("./package.json").version}`,
    "--storage",
    storage,
    "--project",
    project,
    ...modeArgs,
  ];
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return {};
  }
}

function writeJson(file, obj) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(obj, null, 2) + "\n");
}

/** Merge `entry` under `key` ("mcpServers" | "servers") preserving the rest. */
function mergeJsonConfig(file, key, name, entry) {
  const cfg = readJson(file);
  cfg[key] = cfg[key] || {};
  const existing = cfg[key][name];
  cfg[key][name] = entry;
  writeJson(file, cfg);
  return existing;
}

/** codex config.toml: replace-or-append an [mcp_servers.<name>] section. */
function mergeTomlSection(file, name, command, args) {
  let text = "";
  try {
    text = fs.readFileSync(file, "utf8");
  } catch {
    /* new file */
  }
  const header = `[mcp_servers.${name}]`;
  const lines = [
    `${header}`,
    `command = ${JSON.stringify(command)}`,
    `args = ${JSON.stringify(args)}`,
  ];
  const section = lines.join("\n") + "\n";
  const start = text.indexOf(header);
  if (start >= 0) {
    // Replace through the next section header (or EOF).
    let end = text.indexOf("\n[", start + header.length);
    end = end >= 0 ? end + 1 : text.length;
    text = text.slice(0, start) + section + text.slice(end);
  } else {
    text = text.trimEnd() + (text.trimEnd() ? "\n\n" : "") + section;
  }
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}

/** Per-host install descriptors. `file` may be a function(cwd, home). */
const CLIENTS = {
  // Claude Code, project scope (same shape pi uses).
  claude: {
    file: (cwd) => path.join(cwd, ".mcp.json"),
    format: "mcpServers",
    label: "Claude Code (project scope)",
  },
  // Claude Desktop app, user scope.
  "claude-desktop": {
    file: (_cwd, home) =>
      process.platform === "darwin"
        ? path.join(home, "Library", "Application Support", "Claude", "claude_desktop_config.json")
        : process.platform === "win32"
          ? path.join(process.env.APPDATA || home, "Claude", "claude_desktop_config.json")
          : path.join(home, ".config", "Claude", "claude_desktop_config.json"),
    format: "mcpServers",
    label: "Claude Desktop (user scope)",
  },
  // Cursor, project scope.
  cursor: {
    file: (cwd) => path.join(cwd, ".cursor", "mcp.json"),
    format: "mcpServers",
    label: "Cursor (project scope)",
  },
  // Codex CLI, user config (TOML).
  codex: {
    file: (_cwd, home) => path.join(home, ".codex", "config.toml"),
    toml: true,
    label: "Codex CLI (user scope)",
  },
  // pi, project scope.
  pi: {
    file: (cwd) => path.join(cwd, ".mcp.json"),
    format: "mcpServers",
    label: "pi (project scope)",
  },
  // GitHub Copilot: VS Code workspace (.vscode/mcp.json, "servers" key).
  copilot: {
    file: (cwd) => path.join(cwd, ".vscode", "mcp.json"),
    format: "servers",
    label: "GitHub Copilot / VS Code (workspace scope)",
  },
};

function install(args) {
  const cwd = process.cwd();
  const home = os.homedir();
  let clientId = null;
  let storage = STORAGE_DEFAULT;
  let project = "default";
  const modeArgs = [];
  for (let i = 0; i < args.length; i++) {
    const flag = args[i];
    if (["--slim", "--no-vector", "--vector"].includes(flag)) {
      modeArgs.push(flag);
    } else if (["--client", "--storage", "--project"].includes(flag)) {
      const value = args[++i];
      if (!value || value.startsWith("--")) {
        process.stderr.write(`engram-mcp install: ${flag} requires a value\n`);
        process.exit(1);
      }
      if (flag === "--client") clientId = value;
      else if (flag === "--storage") storage = value;
      else project = value;
    } else {
      process.stderr.write(`engram-mcp install: unknown argument ${flag}\n`);
      process.exit(1);
    }
  }
  if (!clientId || !CLIENTS[clientId]) {
    process.stderr.write(
      `engram-mcp install: ${clientId ? `unknown client '${clientId}'` : "missing --client"}\n` +
        `Clients: ${Object.keys(CLIENTS).join(", ")}\n` +
        `Usage: engram-mcp install --client <id> [--storage <path>] [--project <name>] [--slim]\n`
    );
    process.exit(1);
  }
  const client = CLIENTS[clientId];
  const file = client.file(cwd, home);
  const command = "npx";
  const argsForHost = serverArgs(storage, project, modeArgs);

  if (client.toml) {
    mergeTomlSection(file, "engram", command, argsForHost);
  } else {
    const previous = mergeJsonConfig(file, client.format, "engram", {
      type: "stdio",
      command,
      args: argsForHost,
    });
    if (previous) {
      process.stderr.write(`engram-mcp: replaced existing 'engram' entry in ${file}\n`);
    }
  }
  process.stdout.write(
    `engram-mcp: registered 'engram' for ${client.label}\n` +
      `  file:    ${file}\n` +
      `  command: ${command} ${argsForHost.join(" ")}\n` +
      `  storage: ${storage}  (SQLite store is created on first run)\n`
  );
}

module.exports = { install, CLIENTS };
