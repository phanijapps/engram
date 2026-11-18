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
  const selected = new Map();
  const options = {
    '--backend': 'ENGRAM_BACKEND', '--helix-mode': 'ENGRAM_HELIX_MODE',
    '--helix-store': 'ENGRAM_HELIX_STORE', '--helix-endpoint': 'ENGRAM_HELIX_ENDPOINT',
    '--helix-credential-env': 'ENGRAM_HELIX_CREDENTIAL_ENV', '--helix-path': 'ENGRAM_HELIX_PATH',
    '--trusted-root': 'ENGRAM_TRUSTED_ROOT', '--org': 'ENGRAM_ORG',
    '--domain': 'ENGRAM_DOMAIN', '--subdomain': 'ENGRAM_SUBDOMAIN',
    '--embedding-provider': 'ENGRAM_EMBEDDING_PROVIDER',
  };
  function invalid(message) {
    process.stderr.write(`engram-mcp install: ${message}\n`);
    process.exit(1);
  }
  for (let i = 0; i < args.length; i++) {
    const flag = args[i];
    if (["--slim", "--no-vector", "--vector"].includes(flag)) {
      modeArgs.push(flag);
    } else if (["--client", "--storage", "--project", ...Object.keys(options)].includes(flag)) {
      const value = args[++i];
      if (!value || value.startsWith("--")) {
        process.stderr.write(`engram-mcp install: ${flag} requires a value\n`);
        process.exit(1);
      }
      if (flag === "--client") clientId = value;
      else if (flag === "--storage") storage = value;
      else if (flag === "--project") project = value;
      else {
        if (['--backend', '--helix-mode'].includes(flag) && selected.has(flag) && selected.get(flag) !== value)
          invalid('conflicting selection declarations');
        selected.set(flag, value);
        modeArgs.push(flag, value);
      }
    } else {
      process.stderr.write(`engram-mcp install: unsupported argument; credential values must stay in the runtime environment\n`);
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
  if (!modeArgs.some(arg => ['--slim','--no-vector','--vector'].includes(arg)) && process.env.ENGRAM_ENABLE_VECTOR) {
    const choice = process.env.ENGRAM_ENABLE_VECTOR;
    if (['true','1'].includes(choice)) modeArgs.push('--vector');
    else if (['false','0'].includes(choice)) modeArgs.push('--no-vector');
    else invalid('invalid vector selection');
  }
  const backend = selected.get('--backend') || process.env.ENGRAM_BACKEND || 'sqlite';
  if (!['sqlite', 'pgvector', 'pg', 'postgres', 'helixdb'].includes(backend)) invalid('invalid backend selection');
  if (backend !== 'helixdb' && [...selected.keys()].some(flag => flag.startsWith('--helix-')))
    invalid('Helix settings conflict with selected backend');
  for (const [flag, env] of Object.entries(options)) {
    if (flag.startsWith('--helix-') && backend !== 'helixdb') continue;
    if (!selected.has(flag) && process.env[env]) {
      selected.set(flag, process.env[env]); modeArgs.push(flag, process.env[env]);
    }
  }
  if (backend === 'helixdb') {
    const mode = selected.get('--helix-mode');
    if (!['service', 'embedded'].includes(mode)) invalid('Helix requires an explicit service or embedded mode');
    if (!selected.get('--helix-store')) invalid('Helix requires a logical store');
    if (mode === 'service' && (!selected.get('--helix-endpoint') || !selected.get('--helix-credential-env')))
      invalid('Helix service requires an endpoint and credential environment reference');
    if (selected.has('--helix-credential-env') && !/^[A-Za-z_][A-Za-z0-9_]*$/.test(selected.get('--helix-credential-env')))
      invalid('invalid credential environment reference');
    if (selected.has('--helix-endpoint')) {
      try {
        const url = new URL(selected.get('--helix-endpoint'));
        // Exact origin serialization also rejects empty '?' and '#' delimiters,
        // which URL.search/hash expose as empty strings.
        if (!['http:', 'https:'].includes(url.protocol) || !url.hostname ||
            url.username || url.password || url.href !== `${url.origin}/`) throw new Error('unsafe');
      } catch { invalid('endpoint must be an HTTP(S) origin without credentials, path, query or fragment'); }
    }
    if (mode === 'embedded' && (selected.has('--helix-endpoint') || selected.has('--helix-credential-env')))
      invalid('service settings conflict with embedded mode');
    if (mode === 'service' && selected.has('--helix-path')) invalid('embedded path conflicts with service mode');
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
      `  command: ${command} (selected configuration saved)\n` +
      `  storage: ${storage}  (selected store is opened on first run)\n`
  );
}

module.exports = { install, CLIENTS };
