#!/usr/bin/env node
/**
 * Skills parity check (job 002, REQ-007 / AC-005 / DEC-P01 / RISK-003):
 * a deterministic check that fails loudly on any delivered-vs-authored
 * drift. Three assertions:
 *
 *   1. payload parity — when the generated payload (packages/launcher/skills)
 *      exists, its skill set, hygiene, and every file byte must equal the
 *      authored tree (.agents/skills). When the payload has not been
 *      generated (prepack not run — it is gitignored output), the payload
 *      assertions are honestly skipped, not faked.
 *   2. wrapper parity — every committed .pi/agents/engram-*.md must be
 *      byte-identical to its source dialect (.agents/skills/engram-
 *      factory/agents/engram-<role>.pi.md, the project-scope transform is
 *      the identity), and no orphan wrappers may exist (DEC-P01).
 *   3. --selftest — injects drift into a temp fixture and proves this
 *      checker detects it; exits 1 when detection works (0 when it fails
 *      to detect — so CI wiring `; test $? -eq 1` guards the guard).
 *
 * Paths resolve from the script location, so it runs from the repo root and
 * from packages/launcher alike.
 */
'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const REPO = path.resolve(__dirname, '..', '..', '..');
const DEFAULTS = {
  source: path.join(REPO, '.agents', 'skills'),
  payload: path.join(__dirname, '..', 'skills'),
  piAgents: path.join(REPO, '.pi', 'agents'),
};

/** Directories with a SKILL.md — the same derivation the installer uses. */
function skillDirs(root) {
  return fs
    .readdirSync(root, { withFileTypes: true })
    .filter((d) => d.isDirectory() && fs.existsSync(path.join(root, d.name, 'SKILL.md')))
    .map((d) => d.name)
    .sort();
}

const isHygiene = (p) => path.basename(p) === '__pycache__' || p.endsWith('.pyc');

/** Symlink entries under dir, any depth — lstat-based, never followed
 *  (FIND-security-003): a planted link must fail parity as a link, not
 *  compare byte-identical through its target. */
function symlinksUnder(dir) {
  const out = [];
  const walk = (d, prefix) => {
    let entries;
    try {
      entries = fs.readdirSync(d, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = prefix ? `${prefix}/${e.name}` : e.name;
      if (e.isSymbolicLink()) {
        out.push(rel);
        continue;
      }
      if (e.isDirectory()) walk(path.join(d, e.name), rel);
    }
  };
  walk(dir, '');
  return out;
}

/** Relative file list under dir, skipping hygiene entries (source side). */
function filesUnder(dir, { skipHygiene = false } = {}) {
  const out = [];
  const walk = (d, prefix) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = prefix ? `${prefix}/${e.name}` : e.name;
      if (skipHygiene && isHygiene(e.name)) continue;
      if (e.isDirectory()) walk(path.join(d, e.name), rel);
      else out.push(rel);
    }
  };
  walk(dir, '');
  return out;
}

/** Run all parity assertions against explicit roots; returns error list. */
function checkParity({ source, payload, piAgents }) {
  const errors = [];
  const authored = skillDirs(source);

  // --- 1. payload parity (when the generated payload exists) ---
  if (fs.existsSync(payload)) {
    for (const link of symlinksUnder(payload)) {
      errors.push(`payload contains symlink: ${link}`);
    }
    const delivered = fs
      .readdirSync(payload, { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => d.name)
      .sort();
    for (const s of delivered) {
      if (!authored.includes(s)) errors.push(`payload has skill not in authored set: ${s}`);
    }
    for (const s of authored) {
      if (!delivered.includes(s)) errors.push(`authored skill missing from payload: ${s}`);
    }
    for (const bad of filesUnder(payload)) {
      if (isHygiene(bad)) errors.push(`payload hygiene violation: ${bad}`);
    }
    for (const skill of authored) {
      const srcDir = path.join(source, skill);
      const dstDir = path.join(payload, skill);
      if (!fs.existsSync(dstDir)) continue; // already flagged above
      const srcFiles = filesUnder(srcDir, { skipHygiene: true });
      const dstFiles = fs.existsSync(dstDir) ? filesUnder(dstDir) : [];
      for (const f of srcFiles) {
        if (!dstFiles.includes(f)) errors.push(`payload missing file: ${skill}/${f}`);
      }
      for (const f of dstFiles) {
        if (!srcFiles.includes(f)) errors.push(`payload has extra file: ${skill}/${f}`);
      }
      for (const f of srcFiles) {
        if (!dstFiles.includes(f)) continue;
        let a;
        let b;
        try {
          a = fs.readFileSync(path.join(srcDir, f));
          b = fs.readFileSync(path.join(dstDir, f));
        } catch {
          continue; // unreadable (e.g. dangling link): the symlink scan above already failed the run
        }
        if (!a.equals(b)) errors.push(`payload file differs from source: ${skill}/${f}`);
      }
    }
  }

  // --- 1b. authored-tree symlink rejection (FIND-security-003) ---
  // The generator drops symlinks, so an authored link would silently vanish
  // from delivery — flag it here and force the author to resolve it.
  for (const link of symlinksUnder(source)) {
    errors.push(`authored tree contains symlink (not shippable): ${link}`);
  }

  // --- 2. wrapper parity (DEC-P01) ---
  if (fs.existsSync(piAgents)) {
    for (const link of symlinksUnder(piAgents)) {
      errors.push(`pi agents dir contains symlink: .pi/agents/${link}`);
    }
  }
  const dialectDir = path.join(source, 'engram-factory', 'agents');
  const piDialects = fs.existsSync(dialectDir)
    ? fs.readdirSync(dialectDir).filter((f) => /^engram-.+\.pi\.md$/.test(f)).sort()
    : [];
  const committed = fs.existsSync(piAgents)
    ? fs.readdirSync(piAgents).filter((f) => /^engram-.+\.md$/.test(f)).sort()
    : [];
  for (const dialect of piDialects) {
    const destName = dialect.replace(/\.pi\.md$/, '.md');
    const dest = path.join(piAgents, destName);
    if (!fs.existsSync(dest)) {
      errors.push(`pi wrapper missing: .pi/agents/${destName}`);
      continue;
    }
    const a = fs.readFileSync(path.join(dialectDir, dialect));
    const b = fs.readFileSync(dest);
    if (!a.equals(b)) errors.push(`pi wrapper byte drift: .pi/agents/${destName} != ${dialect}`);
  }
  for (const f of committed) {
    const dialect = f.replace(/\.md$/, '.pi.md');
    if (!piDialects.includes(dialect)) {
      errors.push(`orphan pi wrapper without a source dialect: .pi/agents/${f}`);
    }
  }

  return { errors, authored, payloadPresent: fs.existsSync(payload), piDialects };
}

function main() {
  const selftest = process.argv.includes('--selftest');

  if (selftest) {
    // Build a drifted fixture from the real authored tree and prove the
    // checker catches every injected drift kind.
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-parity-selftest-'));
    const source = path.join(root, '.agents', 'skills');
    const payload = path.join(root, 'payload', 'skills');
    const piAgents = path.join(root, '.pi', 'agents');
    fs.cpSync(DEFAULTS.source, source, { recursive: true });
    fs.cpSync(DEFAULTS.source, payload, { recursive: true });
    fs.cpSync(DEFAULTS.piAgents, piAgents, { recursive: true });
    const injected = [];
    // drift 1: payload set drift (extra skill)
    fs.mkdirSync(path.join(payload, 'drift-probe'), { recursive: true });
    fs.writeFileSync(path.join(payload, 'drift-probe', 'SKILL.md'), '---\nname: drift-probe\n---\n');
    injected.push('set drift');
    // drift 2: hygiene violation in the payload
    fs.mkdirSync(path.join(payload, 'engram-index', '__pycache__'), { recursive: true });
    fs.writeFileSync(path.join(payload, 'engram-index', '__pycache__', 'x.pyc'), 'junk');
    injected.push('hygiene drift');
    // drift 3: wrapper byte drift
    const wrapper = path.join(piAgents, 'engram-planner.md');
    fs.writeFileSync(wrapper, fs.readFileSync(wrapper, 'utf8') + '\n<!-- drifted -->\n');
    injected.push('wrapper drift');
    // drift 4: orphan wrapper
    fs.writeFileSync(path.join(piAgents, 'engram-ghost.md'), '---\nname: engram-ghost\n---\n');
    injected.push('orphan wrapper');
    // drift 5: symlink planted in the payload (must fail as a link, never
    // compare byte-identical through its target — FIND-security-003)
    fs.symlinkSync('/etc/passwd', path.join(payload, 'engram-index', 'secret-link'));
    injected.push('payload symlink');

    const { errors } = checkParity({ source, payload, piAgents });
    const caught = {
      'set drift': errors.some((e) => e.includes('drift-probe')),
      'hygiene drift': errors.some((e) => e.includes('hygiene violation')),
      'wrapper drift': errors.some((e) => e.includes('byte drift: .pi/agents/engram-planner.md')),
      'orphan wrapper': errors.some((e) => e.includes('orphan pi wrapper')),
      'payload symlink': errors.some((e) => e.includes('payload contains symlink: engram-index/secret-link')),
    };
    const missed = injected.filter((k) => !caught[k]);
    if (missed.length > 0) {
      console.error(`SELFTEST FAILED — drift not detected: ${missed.join(', ')}`);
      process.exit(2);
    }
    console.log(`selftest: all ${injected.length} injected drift kinds detected (exit 1 by design):`);
    for (const k of injected) console.log(`  detected: ${k}`);
    process.exit(1);
  }

  const { errors, authored, payloadPresent, piDialects } = checkParity(DEFAULTS);
  if (errors.length > 0) {
    console.error(`skills parity: FAIL (${errors.length} difference(s))`);
    for (const e of errors) console.error(`  - ${e}`);
    process.exit(1);
  }
  const payloadLine = payloadPresent
    ? `payload present: ${authored.length}/${authored.length} skills byte-identical, hygiene clean`
    : 'payload not generated (prepack not run) — payload assertions skipped honestly';
  console.log(
    `skills parity: OK (authored ${authored.length} skills; ${payloadLine}; ${piDialects.length} pi wrappers byte-identical)`,
  );
}

main();
