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
    path.join(LAUNCHER_DIR, '..', '..', '.agents', 'skills'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(path.join(c, 'engram-index', 'SKILL.md'))) return c;
  }
  return null;
}

/** Locate the agents source (subagent definitions, per-platform dialects). */
function agentsSource() {
  const candidates = [
    path.join(LAUNCHER_DIR, 'agents'),
    path.join(LAUNCHER_DIR, '..', '..', 'mcp', 'engram-mcp', 'agents'),
  ];
  for (const c of candidates) {
    if (fs.existsSync(path.join(c, 'red-forman.claude.md'))) return c;
  }
  return null;
}

/** Install a subagent definition into `destDir` under `fileName`. */
function copyAgent(destDir, srcName, destName, written) {
  const src = agentsSource();
  if (!src) {
    process.stderr.write('engram-mcp: agents source not found (package incomplete?)\n');
    return;
  }
  fs.mkdirSync(destDir, { recursive: true });
  const to = path.join(destDir, destName);
  fs.copyFileSync(path.join(src, srcName), to);
  written.push(to);
}

/** Map a skill payload agent file to this host's installed name (DEC-P06). */
function dialectDestName(file, dialect) {
  if (dialect === 'codex') {
    return /^engram-.+\.codex\.yaml$/.test(file)
      ? file.replace(/\.codex\.yaml$/, '.yaml')
      : null;
  }
  const re = new RegExp(`^engram-.+\\.${dialect}\\.md$`);
  return re.test(file) ? file.replace(new RegExp(`\\.${dialect}\\.md$`), '.md') : null;
}

/**
 * Generic per-skill agents delivery (DEC-004): every shipped skill may carry
 * an agents/ payload of per-dialect subagent definitions; the files for this
 * host's dialect land in its agents directory. Only the four documented
 * dialect patterns install — anything else (e.g. openai.yaml) travels with
 * the skill folder but is never installed as an agent. The contract pointer
 * is rewritten to `pointerRoot/<skill>/` — project-relative at project scope
 * (`.agents/skills` is the cross-tool default and rewrites to itself), the
 * absolute installed root at user scope — so it resolves from the installed
 * location in both scopes (DEC-P02, REQ-013).
 */
function copySkillAgents(destDir, dialect, pointerRoot, remove, written) {
  const src = skillsSource();
  if (!src) return; // copySkills already reported the missing source
  for (const skill of deriveShippedSkills(src)) {
    const agentsSrc = path.join(src, skill, 'agents');
    if (!fs.existsSync(agentsSrc)) continue;
    const rel = `.agents/skills/${skill}/`;
    const target = `${path.join(pointerRoot, skill).split(path.sep).join('/')}/`;
    for (const file of fs.readdirSync(agentsSrc).sort()) {
      const destName = dialectDestName(file, dialect);
      if (!destName) continue;
      const to = path.join(destDir, destName);
      if (remove) {
        try {
          fs.rmSync(to);
        } catch {}
        continue;
      }
      fs.mkdirSync(destDir, { recursive: true });
      let text = fs.readFileSync(path.join(agentsSrc, file), 'utf8');
      if (text.includes(rel)) {
        text = text.split(rel).join(target);
      }
      fs.writeFileSync(to, text);
      written.push(to);
    }
  }
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

// Skills this installer ships are auto-derived: every top-level directory
// under the source containing SKILL.md ships (DEC-002), so newly authored
// skills flow to delivery with no installer change. Retired names below are
// removed from existing installations — but only directories that still
// carry our SKILL.md (a user's replacement with different frontmatter is
// left untouched).
const RETIRED_SKILLS = ['engram-learn', 'engram-search', 'engram-plan', 'engram-implement'];

/** Every top-level source directory with a SKILL.md ships (DEC-002). */
function deriveShippedSkills(src) {
  return fs
    .readdirSync(src, { withFileTypes: true })
    .filter((d) => d.isDirectory() && fs.existsSync(path.join(src, d.name, 'SKILL.md')))
    .map((d) => d.name)
    .sort();
}

/** Name-tagged removal: only delete a skill dir whose SKILL.md is ours. */
function removeIfOurs(to, skill, written, label) {
  const marker = path.join(to, 'SKILL.md');
  if (!fs.existsSync(marker)) return;
  let ours = false;
  try {
    ours = fs.readFileSync(marker, 'utf8').startsWith(`---\nname: ${skill}\n`);
  } catch {
    ours = false; // unreadable: never delete what we cannot verify
  }
  if (ours) {
    fs.rmSync(to, { recursive: true, force: true });
    written.push(`${to} (${label})`);
  }
}

function copySkills(destRoot, written, remove = false) {
  const src = skillsSource();
  if (!src) {
    process.stderr.write('engram-mcp: skills source not found (package incomplete?)\n');
    return;
  }
  if (remove) {
    // --remove undoes exactly what this installer owns: delivered skills and
    // retired names alike, each name-tagged; foreign content stays (REQ-006).
    for (const skill of [...deriveShippedSkills(src), ...RETIRED_SKILLS]) {
      removeIfOurs(path.join(destRoot, skill), skill, written, 'removed');
    }
    return;
  }
  for (const skill of deriveShippedSkills(src)) {
    const from = path.join(src, skill);
    const to = path.join(destRoot, skill);
    if (!fs.existsSync(path.join(from, 'SKILL.md'))) continue;
    // DEC-P04 self-copy guard: when destination equals source (a project-
    // scope install inside the authored repo), skip — never remove — so the
    // authored tree survives its own install.
    if (path.resolve(from) === path.resolve(to)) continue;
    fs.rmSync(to, { recursive: true, force: true });
    fs.mkdirSync(path.dirname(to), { recursive: true });
    fs.cpSync(from, to, { recursive: true });
    written.push(to);
  }
  for (const skill of RETIRED_SKILLS) {
    removeIfOurs(path.join(destRoot, skill), skill, written, 'retired');
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
  // USER scope installs to the home directory (available everywhere);
  // PROJECT scope installs into the current repo (committed, per-team).
  claude: {
    label: 'Claude Code',
    install(home, remove, written, cwd, scope) {
      const root = scope === 'project' ? path.join(cwd, '.claude') : path.join(home, '.claude');
      copySkills(path.join(root, 'skills'), written, remove);
      const agentDir = path.join(root, 'agents');
      if (remove) { try { fs.rmSync(path.join(agentDir, 'red-forman.md')); } catch {} }
      else copyAgent(agentDir, 'red-forman.claude.md', 'red-forman.md', written);
      copySkillAgents(agentDir, 'claude', scope === 'project' ? path.join('.claude', 'skills') : path.join(root, 'skills'), remove, written);
      const settings = path.join(root, 'settings.json');
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
    install(home, remove, written, cwd, scope) {
      // Skills: the cross-tool .agents/skills dir (project) or ~/.agents/skills (user).
      copySkills(
        scope === 'project' ? path.join(cwd, '.agents', 'skills') : path.join(home, '.agents', 'skills'),
        written,
        remove,
      );
      // Agents: Codex discovers agents at user level only (~/.codex/agents,
      // YAML form) — no documented project-agent location.
      const agentDir = path.join(home, '.codex', 'agents');
      if (remove) { try { fs.rmSync(path.join(agentDir, 'red-forman.yaml')); } catch {} }
      else {
        copyAgent(agentDir, 'red-forman.codex.yaml', 'red-forman.yaml', written);
        if (scope === 'project') {
          process.stdout.write('  note: Codex agents are user-scope (no project-agent discovery)\n');
        }
      }
      copySkillAgents(
        agentDir,
        'codex',
        scope === 'project' ? path.join('.agents', 'skills') : path.join(home, '.agents', 'skills'),
        remove,
        written,
      );
      const recall = hookScript('recall-context.cjs');
      const stale = hookScript('stale-index.cjs');
      if (scope === 'project') {
        // Project hooks live in <repo>/.codex/hooks.json (JSON, not TOML).
        const file = path.join(cwd, '.codex', 'hooks.json');
        if (remove) {
          try { fs.rmSync(file); } catch {}
        } else {
          writeJson(file, {
            description: 'engram agent bundle (skills + hooks + subagent)',
            hooks: {
              UserPromptSubmit: [
                { hooks: [{ type: 'command', command: `node "${recall}"`, async: true, statusMessage: 'engram recall' }] },
              ],
              Stop: [{ hooks: [{ type: 'command', command: `node "${stale}"` }] }],
              SessionStart: [
                { matcher: 'startup|resume', hooks: [{ type: 'command', command: `node "${recall}"`, additionalContextLimit: 2000 }] },
              ],
            },
          });
          written.push(file);
        }
      } else {
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
      }
      if (!remove) {
        process.stdout.write('  note: run /hooks inside Codex to trust the new hooks\n');
      }
    },
  },
  copilot: {
    label: 'GitHub Copilot',
    install(home, remove, written, cwd, scope) {
      // Skills: .agents/skills (project, cross-tool) or ~/.agents/skills (user).
      copySkills(
        scope === 'project' ? path.join(cwd, '.agents', 'skills') : path.join(home, '.agents', 'skills'),
        written,
        remove,
      );
      // Agents: repo-scoped either way (.github/agents/).
      const agentDir = path.join(cwd || process.cwd(), '.github', 'agents');
      if (remove) { try { fs.rmSync(path.join(agentDir, 'red-forman.md')); } catch {} }
      else copyAgent(agentDir, 'red-forman.copilot.md', 'red-forman.md', written);
      copySkillAgents(
        agentDir,
        'copilot',
        scope === 'project' ? path.join('.agents', 'skills') : path.join(home, '.agents', 'skills'),
        remove,
        written,
      );
      // Per-prompt injection is not available to Copilot command hooks
      // (output dropped by design) — sessionStart only. Honest limitation.
      const file =
        scope === 'project'
          ? path.join(cwd, '.github', 'hooks', 'engram.json')
          : path.join(home, '.copilot', 'hooks', 'engram.json');
      if (remove) {
        try { fs.rmSync(file); } catch {}
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
      if (scope === 'project' && !remove) {
        process.stdout.write('  note: commit .github/ and .agents/ so the team gets the bundle\n');
      }
    },
  },
  pi: {
    label: 'pi',
    install(home, remove, written, cwd, scope) {
      // Skills: pi reads the cross-tool .agents/skills at project level
      // (same dir codex/copilot read) or ~/.pi/agent/skills at user level.
      copySkills(
        scope === 'project' ? path.join(cwd, '.agents', 'skills') : path.join(home, '.pi', 'agent', 'skills'),
        written,
        remove,
      );
      // Agents: .pi/agents/ (project) or ~/.pi/agent/agents/ (user).
      const agentDir =
        scope === 'project' ? path.join(cwd, '.pi', 'agents') : path.join(home, '.pi', 'agent', 'agents');
      if (remove) { try { fs.rmSync(path.join(agentDir, 'red-forman.md')); } catch {} }
      else copyAgent(agentDir, 'red-forman.pi.md', 'red-forman.md', written);
      copySkillAgents(
        agentDir,
        'pi',
        scope === 'project' ? path.join('.agents', 'skills') : path.join(home, '.pi', 'agent', 'skills'),
        remove,
        written,
      );
      // Extensions are pi's hook mechanism: .pi/extensions/ (project)
      // or ~/.pi/agent/extensions/ (user).
      const ext =
        scope === 'project'
          ? path.join(cwd, '.pi', 'extensions', 'engram-recall.cjs')
          : path.join(home, '.pi', 'agent', 'extensions', 'engram-recall.cjs');
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
  const cwd = process.cwd();
  let target = null;
  let remove = false;
  let scope = 'user';
  let storage = process.env.ENGRAM_STORAGE || path.join(home, '.engram');
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--agent') target = args[++i];
    else if (args[i] === '--remove') remove = true;
    else if (args[i] === '--scope') scope = args[++i];
    else if (args[i] === '--storage') storage = args[++i];
  }
  if (scope !== 'user' && scope !== 'project') {
    process.stderr.write(`engram-mcp install: --scope must be 'user' or 'project'\n`);
    process.exit(1);
  }
  if (!target || (target !== 'all' && !HOSTS[target])) {
    process.stderr.write(
      `engram-mcp install: ${target ? `unknown agent '${target}'` : 'missing --agent'}\n` +
        `Agents: ${Object.keys(HOSTS).join(', ')}, all\n` +
        `Usage: engram-mcp install --agent <id> [--scope user|project] [--remove] [--storage <path>]\n`,
    );
    process.exit(1);
  }
  const ids = target === 'all' ? Object.keys(HOSTS) : [target];
  process.stdout.write(
    `${remove ? 'Removing' : 'Installing'} engram agent bundle (skills + hooks + subagents)\n` +
      `  scope: ${scope}${scope === 'project' ? ` (${cwd})` : ''}\n` +
      `  storage: ${storage}\n\n`,
  );
  if (target === 'all' && !remove) {
    // Honest limitation (REQ-005): claude-desktop has no skills/agents
    // filesystem — it only takes MCP registration, which is a different
    // command. Name it instead of staying silent.
    process.stdout.write(
      'note: claude-desktop cannot host skills or subagents — MCP registration only\n' +
        '      (engram-mcp install --client claude-desktop)\n\n',
    );
  }
  for (const id of ids) {
    const host = HOSTS[id];
    process.stdout.write(`[${host.label}]\n`);
    const written = [];
    try {
      host.install(home, remove, written, cwd, scope);
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
