#!/usr/bin/env node
/**
 * engram stale-index hook (RFC-0022) — Stop-event reminder.
 *
 * When the agent finishes responding, check whether the engram index is
 * stale (many files changed since the last scan_repo). If so, inject a
 * ONE-LINE reminder. Never rescans here — the agent decides.
 *
 * Staleness = files changed (git) since last_index_at + uncommitted edits.
 * Env: ENGRAM_HOOKS=0 kill switch; ENGRAM_STALE_THRESHOLD (default 25).
 * Fails open: exit 0 silently on every error path.
 */
'use strict';

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createRequire } = require('node:module');

function resolveCandidates() {
  // Ordered: env override → this package's platform dep → PATH → dev build.
  // The CALLER falls through to the next candidate when a binary exits 2
  // (usage error = a stale pre-oneshot binary that lacks the subcommand —
  // observed in dev trees with mixed node_modules).
  const out = [];
  if (process.env.ENGRAM_MCP_BIN) out.push(process.env.ENGRAM_MCP_BIN);
  // Same resolution discipline as recall-context.cjs / bin.js: this file's
  // own dependency tree first (version lockstep), then PATH, then dev.
  const candidates = [
    ['linux', 'x64', '@engramlabs/mcp-linux-x64-gnu'],
    ['linux', 'arm64', '@engramlabs/mcp-linux-arm64-gnu'],
    ['darwin', 'arm64', '@engramlabs/mcp-darwin-arm64'],
    ['darwin', 'x64', '@engramlabs/mcp-darwin-x64'],
    ['win32', 'x64', '@engramlabs/mcp-win32-x64'],
  ];
  const hit = candidates.find(([p, a]) => p === process.platform && a === process.arch);
  const binName = 'engram-mcp' + (process.platform === 'win32' ? '.exe' : '');
  if (hit) {
    try {
      const req = createRequire(__filename);
      const manifest = req.resolve(`${hit[2]}/package.json`);
      const bin = path.join(path.dirname(manifest), binName);
      if (fs.existsSync(bin)) out.push(bin);
    } catch {}
  }
  const which = spawnSync(
    process.platform === 'win32' ? 'where' : 'which',
    ['engram-mcp'],
    { encoding: 'utf8', timeout: 1000 },
  );
  if (which.status === 0) {
    const p = String(which.stdout).trim().split('\n')[0];
    if (p && fs.existsSync(p)) out.push(p);
  }
  const dev = path.join(os.homedir(), 'projects', 'mem-alpha', 'target', 'release', 'engram-mcp');
  try {
    if (fs.existsSync(dev)) out.push(dev);
  } catch {}
  return out;
}

function git(args, cwd) {
  const r = spawnSync('git', args, { cwd, encoding: 'utf8', timeout: 3000 });
  return r.status === 0 ? r.stdout : '';
}

function main() {
  if (process.env.ENGRAM_HOOKS === '0') process.exit(0);

  let cwd = process.cwd();
  try {
    const j = JSON.parse(stdinCache);
    if (j.cwd) cwd = j.cwd;
  } catch {}

  const storage = process.env.ENGRAM_STORAGE || path.join(os.homedir(), '.engram');
  const project =
    process.env.ENGRAM_PROJECT || path.basename(path.resolve(cwd)) || 'default';
  const threshold = parseInt(process.env.ENGRAM_STALE_THRESHOLD || '25', 10);

  const candidates = resolveCandidates();
  if (candidates.length === 0) process.exit(0);

  let res = null;
  for (const cand of candidates) {
    const r = spawnSync(
      cand,
      ['status', '--storage', storage, '--project', project, '--json'],
      { timeout: 3000, encoding: 'utf8' },
    );
    if (r.status === 2) continue; // stale binary — next candidate
    res = r;
    break;
  }
  if (!res || res.error || res.status !== 0) process.exit(0);
  let status;
  try {
    const line = res.stdout.split('\n').reverse().find((l) => l.trim().startsWith('{'));
    status = JSON.parse(line);
  } catch {
    process.exit(0);
  }
  if (!status.ok || !status.last_index_at) process.exit(0); // never indexed: skills handle that

  // Files changed since the index: committed + working-tree.
  const since = new Date(status.last_index_at * 1000).toISOString();
  const committed = git(['log', '--name-only', '--pretty=format:', '--since=' + since], cwd);
  const dirty = git(['status', '--porcelain'], cwd);
  const changed = new Set(
    (committed + '\n' + dirty)
      .split('\n')
      .map((l) => l.trim().replace(/^"|"$/g, ''))
      .filter(Boolean),
  );

  if (changed.size < threshold) process.exit(0);

  process.stdout.write(
    JSON.stringify({
      hookSpecificOutput: {
        hookEventName: 'Stop',
        additionalContext:
          `engram index is stale (${changed.size} files changed since the last scan) — ` +
          `run scan_repo at a natural pause to refresh it.`,
      },
    }),
  );
  process.exit(0);
}

const stdinCache = (() => {
  try {
    return fs.readFileSync(0, 'utf8');
  } catch {
    return '';
  }
})();
main();