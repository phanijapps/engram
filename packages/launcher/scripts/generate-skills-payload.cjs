#!/usr/bin/env node
/**
 * Skills payload generator (DEC-001/DEC-P03): derives the npm payload at
 * packages/launcher/skills from the single authored tree .agents/skills.
 *
 *   - regenerate declared canonical agent wrappers and Pi mirrors first
 *   - whole-folder copy of authored Engram skills selected by skill-scope.cjs
 *   - hygiene excludes: __pycache__ directories and *.pyc files never ship
 *   - wired as npm `prepack`, so `npm pack` always packs a fresh payload
 *
 * The payload is generated output (gitignored) — never hand-edit it; edit
 * .agents/skills and regenerate. Tests call this script directly so nothing
 * depends on pack lifecycle semantics.
 */
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { distributedSkillDirs } = require('./skill-scope.cjs');

const SOURCE = path.resolve(__dirname, '..', '..', '..', '.agents', 'skills');
const DEST = path.resolve(__dirname, '..', 'skills');

function isHygiene(src) {
  const base = path.basename(src);
  return base === '__pycache__' || base.endsWith('.pyc');
}

/** Symlinks never ship (FIND-security-003): a planted link would ride the
 *  payload verbatim — installed into every user's skills dir and read
 *  through by prompt-executed agents — so the filter lstat-rejects them
 *  instead of dereferencing or preserving them. */
function isSymlink(src) {
  try {
    return fs.lstatSync(src).isSymbolicLink();
  } catch {
    return false;
  }
}

function main() {
  if (!fs.existsSync(path.join(SOURCE, 'engram-index', 'SKILL.md'))) {
    process.stderr.write(`generate-skills-payload: authored tree not found at ${SOURCE}\n`);
    process.exit(1);
  }
  // Canonical role contracts own wrappers and the repository Pi copies.
  // Generate these before copying, so prepack cannot publish stale dialects.
  const factory = path.join(SOURCE, 'engram-factory');
  const manifest = path.join(factory, 'agents', 'roles.json');
  const canonicalDeclared = fs.existsSync(manifest);
  let bundleDigest;
  if (canonicalDeclared) {
    const generator = path.join(factory, 'scripts', 'generate_agents.py');
    const result = spawnSync('python3', [generator, '--skill-root', factory,
      '--repo-root', path.resolve(SOURCE, '..', '..')], { encoding: 'utf8', timeout: 60000, env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } });
    if (result.error || result.status !== 0) {
      process.stderr.write(`generate-skills-payload: canonical agents generation failed: ${result.error || result.stderr || result.stdout}\n`);
      process.exit(1);
    }
    try {
      bundleDigest = JSON.parse(result.stdout).bundle_sha256;
      if (!/^[a-f0-9]{64}$/.test(bundleDigest)) throw new Error('missing bundle_sha256');
    } catch (error) {
      process.stderr.write(`generate-skills-payload: invalid canonical generation identity: ${error.message}\n`);
      process.exit(1);
    }
  } else {
    process.stderr.write('canonical agents: no roles manifest declared — generation skipped\n');
  }
  const skills = distributedSkillDirs(SOURCE);
  fs.rmSync(DEST, { recursive: true, force: true });
  fs.mkdirSync(DEST, { recursive: true });
  for (const skill of skills) {
    fs.cpSync(path.join(SOURCE, skill), path.join(DEST, skill), {
      recursive: true,
      filter: (src) => !isHygiene(src) && !isSymlink(src),
    });
  }
  // Check the copied candidate, not only the source observed before copying.
  // A canonical edit during the copy must not publish mixed instruction revisions.
  if (canonicalDeclared) {
    for (const [label, skillRoot] of [['source', factory], ['copied', path.join(DEST, 'engram-factory')]]) {
      const checked = spawnSync('python3', [path.join(skillRoot, 'scripts', 'generate_agents.py'),
        '--skill-root', skillRoot, '--repo-root', path.resolve(SOURCE, '..', '..'),
        '--check', '--expected-digest', bundleDigest],
      { encoding: 'utf8', timeout: 60000, env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } });
      if (checked.error || checked.status !== 0) {
        process.stderr.write(`generate-skills-payload: ${label} canonical agents check failed: ${checked.error || checked.stderr || checked.stdout}\n`);
        process.exit(1);
      }
    }
  }
  process.stderr.write(`generated skills payload: ${skills.length} skills from ${SOURCE}\n`);
  process.stderr.write(`  ${skills.join(', ')}\n`);
}

main();
