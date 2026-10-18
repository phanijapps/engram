/**
 * Agent bundle installer (RFC-0022 Phase 3): skills + hooks + MCP
 * registration for a host, in one command.
 *
 *   engram-mcp install --agent claude|codex|copilot|pi|all [--remove]
 *
 * What each host gets:
 *   claude  ~/.claude/skills/engram-*  + ~/.claude/settings.json hooks
 *           (UserPromptSubmit async, Stop, SessionStart)
 *   codex   ~/.agents/skills/engram-*  + ~/.codex/hooks.json (UserPromptSubmit,
 *           Stop, SessionStart) — Codex requires /hooks trust review
 *   copilot ~/.agents/skills/engram-* (shared with codex) + ~/.copilot/hooks/
 *           engram.json (sessionStart only — per-prompt injection is not
 *           available to Copilot command hooks; documented limitation)
 *   pi      ~/.pi/agent/skills/engram-* + ~/.pi/agent/extensions/engram-recall.cjs
 *
 * Fail-open philosophy: install reports everything it writes; --remove
 * undoes exactly the entries this installer owns (name-tagged).
 */
'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const LAUNCHER_DIR = __dirname;
const TAG = 'engram (engram-mcp installer)';

/** Locate the skills source: installed package first, dev checkout second. */
function skillsSource() {
  const candidates = [
    path.join(LAUNCHER_DIR, 'skills'),
    path.join(LAUNCHER_DIR, '..', '..', 'mcp', 'engram-mcp', 'skills'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(path.join(c, 'engram-index', 'SKILL.md'))) return c;
  }
  return null;
}

function hookScript(name) {
  return path.join(LAUNCHER_DIR, 'hooks', name);
}

function nodeCommand(script) {
  return { command: 'node', args: [script] };
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return {};
  }
}

function writeJson(file, obj) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(obj, null, 2) + '\n');
}

function writeTomlSection(file, section, body) {
  // Appends a TOML section ([[hooks.UserPromptSubmit]] style) or removes
  // previously-installed tagged sections. Simple line-based append; Codex
  // merges hooks.json + inline tables, and we own only our tagged block.
  fs.mkdirSync(path.dirname(file), { recursive: true });
  let text = '';
  try {
    text = fs.readFileSync(file, 'utf8');
  } catch {}
  const begin = `# BEGIN ${TAG}`;
  const end = `# END ${TAG}`;
  const startIdx = text.indexOf(begin);
  if (startIdx >= 0) {
    const endIdx = text.indexOf(end);
    text =
      text.slice(0, startIdx) +
      text.slice(endIdx >= 0 ? endIdx + end.length : startIdx + begin.length);
  }
  if (body) {
    text = text.trimEnd() + '\n\n' + begin + '\n' + body.trim() + '\n' + end + '\n';
  } else {
    text = text.trimEnd() + (text ? '\n' : '');
  }
  fs.writeFileSync(file, text);
}

function copySkills(destRoot, written) {
  const src = skillsSource();
  if (!src) {
    process.stderr.write('engram-mcp: skills source not found (package incomplete?)\n');
    return;
  }
  for (const skill of ['engram-index', 'engram-learn', 'engram-search', 'engram-plan', 'engram-implement']) {
    const from = path.join(src, skill);
    const to = path.join(destRoot, skill);
    if (!fs.existsSync(path.join(from, 'SKILL.md'))) continue;
    fs.rmSync(to, { recursive: true, force: true });
    fs.mkdirSync(path.dirname(to), { recursive: true });
    fs.cpSync(from, to, { recursive: true });
    written.push(to);
  }
}

/** Merge our hook entries into a Claude-style settings.json. */
function mergeClaudeHooks(file, entries, remove) {
  const cfg = readJson(file);
  cfg.hooks = cfg.hooks || {};
  for (const [event, spec] of entries) {
    const list = (cfg.hooks[event] = Array.isArray(cfg.hooks[event]) ? cfg.hooks[event] : []);
    // Drop our previous entries (idempotent re-install), keep foreign ones.
    cfg.hooks[event] = list.filter(
      (g) => !(g._engram || (g.hooks || []).some((h) => (h.command || '').includes('engram-mcp installer') || (h.command || '').includes(path.join('hooks', 'recall-context')) || (h.command || '').includes(path.join('hooks', 'stale-index')))),
    );
    if (!remove) {
      cfg.hooks[event].push({ _engram: true, hooks: spec.map((s) => ({ type: 'command', ...s })) });
    }
    if (cfg.hooks[event].length === 0) delete cfg.hooks[event];
  }
  if (Object.keys(cfg.hooks).length === 0) delete cfg.hooks;
  writeJson(file, cfg);
}

const HOSTS = {
  claude: {
    label: 'Claude Code',
    skillsDir: (home) => path.join(home, '.claude', 'skills'),
    install(home, remove, written) {
      copySkills(this.skillsDir(home), written);
      const settings = path.join(home, '.claude', 'settings.json');
      const recall = hookScript('recall-context.cjs');
      const stale = hookScript('stale-index.cjs');
      mergeClaudeHooks(
        settings,
        [
          ['UserPromptSubmit', [{ command: `node ${recall}`, async: true }]],
          ['Stop', [{ command: `node ${stale}` }]],
          ['SessionStart', [{ command: `node ${recall}` }]],
        ],
        remove,
      );
      written.push(settings);
    },
  },
  codex: {
    label: 'Codex',
    skillsDir: (home) => path.join(home, '.agents', 'skills'),
    install(home, remove, written) {
      copySkills(this.skillsDir(home), written);
      const recall = hookScript('recall-context.cjs');
      const stale = hookScript('stale-index.cjs');
      const body = remove
        ? null
        : [
            '[[hooks.UserPromptSubmit]]',
            '  [[hooks.UserPromptSubmit.hooks]]',
            '  type = "command"',
            `  command = 'node "${recall}"'`,
            '  async = true',
            '  statusMessage = "engram recall"',
            '',
            '[[hooks.Stop]]',
            '  [[hooks.Stop.hooks]]',
            '  type = "command"',
            `  command = 'node "${stale}"'`,
            '',
            '[[hooks.SessionStart]]',
            '  matcher = "startup|resume"',
            '  [[hooks.SessionStart.hooks]]',
            '  type = "command"',
            `  command = 'node "${recall}"'`,
            '  additionalContextLimit = 2000',
          ].join('\n');
      writeTomlSection(path.join(home, '.codex', 'config.toml'), 'hooks', body);
      written.push(path.join(home, '.codex', 'config.toml'));
      if (!remove) {
        process.stdout.write('  note: run /hooks inside Codex to trust the new hooks\n');
      }
    },
  },
  copilot: {
    label: 'GitHub Copilot',
    skillsDir: (home) => path.join(home, '.agents', 'skills'),
    install(home, remove, written) {
      copySkills(this.skillsDir(home), written);
      // Per-prompt injection is not available to Copilot command hooks
      // (output dropped by design) — sessionStart only. Honest limitation.
      const file = path.join(home, '.copilot', 'hooks', 'engram.json');
      if (remove) {
        try {
          fs.rmSync(file);
        } catch {}
      } else {
        writeJson(file, {
          version: 1,
          hooks: {
            sessionStart: [
              {
                hooks: [{ type: 'bash', command: `node ${hookScript('recall-context.cjs')}` }],
              },
            ],
          },
        });
        written.push(file);
      }
    },
  },
  pi: {
    label: 'pi',
    skillsDir: (home) => path.join(home, '.pi', 'agent', 'skills'),
    install(home, remove, written) {
      copySkills(this.skillsDir(home), written);
      const ext = path.join(home, '.pi', 'agent', 'extensions', 'engram-recall.cjs');
      if (remove) {
        try {
          fs.rmSync(ext);
        } catch {}
      } else {
        fs.mkdirSync(path.dirname(ext), { recursive: true });
        fs.copyFileSync(path.join(LAUNCHER_DIR, 'hooks', 'engram-recall-pi.cjs'), ext);
        written.push(ext);
      }
    },
  },
};

function agentInstall(args) {
  const home = os.homedir();
  let target = null;
  let remove = false;
  let storage = process.env.ENGRAM_STORAGE || path.join(home, '.engram');
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--agent') target = args[++i];
    else if (args[i] === '--remove') remove = true;
    else if (args[i] === '--storage') storage = args[++i];
  }
  if (!target || (target !== 'all' && !HOSTS[target])) {
    process.stderr.write(
      `engram-mcp install: ${target ? `unknown agent '${target}'` : 'missing --agent'}\n` +
        `Agents: ${Object.keys(HOSTS).join(', ')}, all\n` +
        `Usage: engram-mcp install --agent <id> [--remove] [--storage <path>]\n`,
    );
    process.exit(1);
  }
  const ids = target === 'all' ? Object.keys(HOSTS) : [target];
  process.stdout.write(
    `${remove ? 'Removing' : 'Installing'} engram agent bundle (skills + hooks)\n` +
      `  storage: ${storage}\n\n`,
  );
  for (const id of ids) {
    const host = HOSTS[id];
    process.stdout.write(`[${host.label}]\n`);
    const written = [];
    try {
      host.install(home, remove, written);
      for (const w of written) process.stdout.write(`  ${remove ? 'cleaned' : 'wrote'}: ${w}\n`);
      if (id === 'copilot' && !remove) {
        process.stdout.write(
          '  note: Copilot command hooks cannot inject per-prompt context\n' +
            '        (platform limitation) — session-start recall only\n',
        );
      }
    } catch (e) {
      process.stderr.write(`  error: ${e.message}\n`);
    }
    process.stdout.write('\n');
  }
  process.stdout.write('Done. MCP server registration: engram-mcp install --client <id>\n');
}

module.exports = { agentInstall, HOSTS };
