#!/usr/bin/env node
/**
 * Skills payload generator (DEC-001/DEC-P03): derives the npm payload at
 * packages/launcher/skills from the single authored tree .agents/skills.
 *
 *   - whole-folder copy of every top-level directory containing SKILL.md
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

const SOURCE = path.resolve(__dirname, '..', '..', '..', '.agents', 'skills');
const DEST = path.resolve(__dirname, '..', 'skills');

/** Every top-level source directory with a SKILL.md ships (DEC-002). */
function deriveSkillDirs(root) {
  return fs
    .readdirSync(root, { withFileTypes: true })
    .filter((d) => d.isDirectory() && fs.existsSync(path.join(root, d.name, 'SKILL.md')))
    .map((d) => d.name)
    .sort();
}

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
  const skills = deriveSkillDirs(SOURCE);
  fs.rmSync(DEST, { recursive: true, force: true });
  fs.mkdirSync(DEST, { recursive: true });
  for (const skill of skills) {
    fs.cpSync(path.join(SOURCE, skill), path.join(DEST, skill), {
      recursive: true,
      filter: (src) => !isHygiene(src) && !isSymlink(src),
    });
  }
  process.stdout.write(`generated skills payload: ${skills.length} skills from ${SOURCE}\n`);
  process.stdout.write(`  ${skills.join(', ')}\n`);
}

main();
