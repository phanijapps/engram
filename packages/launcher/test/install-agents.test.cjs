/**
 * Agent-bundle installer tests (job 002): auto-derived skill delivery,
 * hosts/scopes, unknown host, fixture-skill flow-through, double-install
 * idempotency, name-tagged --remove, and the DEC-P04 self-copy guard.
 *
 * Black-box: every case spawns bin.js with HOME pointed at a temp home and
 * cwd at a temp project — the real user home and the real repo are never
 * written. The skills payload under test is a controlled copy so the
 * auto-derivation (not the repo state) is what the assertions exercise.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const LAUNCHER = path.join(__dirname, '..');
const REPO = path.join(LAUNCHER, '..', '..');
const AUTHORED = path.join(REPO, '.agents', 'skills');
const AUTHORED_SKILLS = fs
  .readdirSync(AUTHORED, { withFileTypes: true })
  .filter((d) => d.isDirectory() && fs.existsSync(path.join(AUTHORED, d.name, 'SKILL.md')))
  .map((d) => d.name)
  .sort();
assert.ok(AUTHORED_SKILLS.length >= 6, `authored set too small: ${AUTHORED_SKILLS}`);

/** A throwaway sandbox: a launcher copy (payload optional) + home + project. */
function sandbox(name, { payload = true } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `engram-agents-${name}-`));
  const pkg = path.join(root, 'pkg');
  fs.cpSync(LAUNCHER, pkg, { recursive: true });
  fs.rmSync(path.join(pkg, 'skills'), { recursive: true, force: true });
  fs.rmSync(path.join(pkg, 'test'), { recursive: true, force: true });
  if (payload) copyAuthored(pkg, AUTHORED_SKILLS);
  const home = path.join(root, 'home');
  const proj = path.join(root, 'proj');
  fs.mkdirSync(home);
  fs.mkdirSync(proj);
  return { root, pkg, home, proj };
}

/** Populate a launcher payload from the authored tree (subset or all). */
function copyAuthored(pkg, skills) {
  const destRoot = path.join(pkg, 'skills');
  fs.mkdirSync(destRoot, { recursive: true });
  for (const s of skills) {
    fs.cpSync(path.join(AUTHORED, s), path.join(destRoot, s), { recursive: true });
  }
}

/** Run the real CLI with an isolated HOME and cwd. */
function run(pkg, args, { cwd, home }) {
  return spawnSync('node', [path.join(pkg, 'bin.js'), 'install', '--agent', ...args], {
    cwd,
    encoding: 'utf8',
    timeout: 60000,
    env: { ...process.env, HOME: home },
  });
}

/** Sorted relative-path listing of everything under `dir`. */
function tree(dir) {
  const out = [];
  const walk = (d, prefix) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = prefix ? `${prefix}/${e.name}` : e.name;
      out.push(rel);
      if (e.isDirectory()) walk(path.join(d, e.name), rel);
    }
  };
  walk(dir, '');
  return out;
}

const sha = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

// ------------------------------------------------------------ AC-001 hosts
for (const [host, rootOf] of [
  ['claude', (h) => path.join(h, '.claude', 'skills')],
  ['codex', (h) => path.join(h, '.agents', 'skills')],
  ['copilot', (h) => path.join(h, '.agents', 'skills')],
  ['pi', (h) => path.join(h, '.pi', 'agent', 'skills')],
]) {
  test(`AC-001 ${host}: every authored skill lands with its full folder (user scope)`, () => {
    const { pkg, home, proj } = sandbox(host);
    const res = run(pkg, [host], { cwd: proj, home });
    assert.equal(res.status, 0, `stderr: ${res.stderr}`);
    const root = rootOf(home);
    for (const skill of AUTHORED_SKILLS) {
      assert.ok(
        fs.existsSync(path.join(root, skill, 'SKILL.md')),
        `${skill}/SKILL.md missing under ${root}`,
      );
    }
    // Whole folder travels: factory carries its controller + dialects.
    assert.ok(fs.existsSync(path.join(root, 'engram-factory', 'scripts', 'factory.py')));
    assert.ok(fs.existsSync(path.join(root, 'engram-factory', 'agents', 'engram-builder.pi.md')));
    // Delivered set equals authored set — nothing extra, nothing missing.
    assert.deepEqual(
      fs.readdirSync(root).sort(),
      AUTHORED_SKILLS,
      `${host}: delivered set != authored set`,
    );
  });
}

test('AC-001 project scope: skills land in the repo, per host layout', () => {
  const { pkg, home, proj } = sandbox('project-scope');
  for (const [host, rel] of [
    ['claude', path.join('.claude', 'skills')],
    ['codex', path.join('.agents', 'skills')],
    ['copilot', path.join('.agents', 'skills')],
    ['pi', path.join('.agents', 'skills')],
  ]) {
    const res = run(pkg, [host, '--scope', 'project'], { cwd: proj, home });
    assert.equal(res.status, 0, `${host} stderr: ${res.stderr}`);
    for (const skill of AUTHORED_SKILLS) {
      assert.ok(
        fs.existsSync(path.join(proj, rel, skill, 'SKILL.md')),
        `${host}: ${skill} missing at ${rel}`,
      );
    }
  }
});

test('AC-001 unknown host exits non-zero with usage', () => {
  const { pkg, home, proj } = sandbox('unknown');
  const res = run(pkg, ['nosuch'], { cwd: proj, home });
  assert.notEqual(res.status, 0, 'unknown host must fail');
  assert.match(res.stderr, /unknown agent 'nosuch'/);
  assert.match(res.stderr, /Usage: engram-mcp install --agent/);
});

// ------------------------------------------------- AC-002 fixture skill flow
test('AC-002 a fixture skill added to the source ships with no installer change', () => {
  const { pkg, home, proj } = sandbox('fixture');
  const fixture = path.join(pkg, 'skills', 'fixture-probe');
  fs.mkdirSync(fixture, { recursive: true });
  fs.writeFileSync(
    path.join(fixture, 'SKILL.md'),
    '---\nname: fixture-probe\ndescription: Fixture skill proving auto-derived delivery; trigger on install rerun.\n---\n\n# fixture-probe\n\nFixture body.\n',
  );
  const before = sha(path.join(pkg, 'agent-install.js'));
  const res = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  assert.ok(
    fs.existsSync(path.join(home, '.pi', 'agent', 'skills', 'fixture-probe', 'SKILL.md')),
    'fixture skill not delivered',
  );
  assert.equal(sha(path.join(pkg, 'agent-install.js')), before, 'installer file changed');
});

test('DEC-002 a payload directory without SKILL.md does not ship', () => {
  const { pkg, home, proj } = sandbox('noskillmd');
  fs.mkdirSync(path.join(pkg, 'skills', 'not-a-skill'), { recursive: true });
  fs.writeFileSync(path.join(pkg, 'skills', 'not-a-skill', 'README.md'), 'junk\n');
  const res = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  const root = path.join(home, '.pi', 'agent', 'skills');
  assert.ok(!fs.existsSync(path.join(root, 'not-a-skill')));
  assert.deepEqual(fs.readdirSync(root).sort(), AUTHORED_SKILLS);
});

// ------------------------------------------------------ AC-012 double install
test('AC-012 second install writes the identical set — no duplicates, no orphans', () => {
  const { pkg, home, proj } = sandbox('double');
  const first = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(first.status, 0, `stderr: ${first.stderr}`);
  const agentRoot = path.join(home, '.pi', 'agent');
  const afterFirst = tree(agentRoot);
  const second = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(second.status, 0, `stderr: ${second.stderr}`);
  assert.deepEqual(tree(agentRoot), afterFirst, 'second run changed the written set');
});

// --------------------------------------------------- AC-009 --remove semantics
test('AC-009 --remove removes owned and retired skills, keeps foreign content', () => {
  const { pkg, home, proj } = sandbox('remove');
  const install = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(install.status, 0, `stderr: ${install.stderr}`);
  const root = path.join(home, '.pi', 'agent', 'skills');
  // A user-authored foreign skill (different frontmatter name) must survive.
  const foreign = path.join(root, 'user-skill');
  fs.mkdirSync(foreign, { recursive: true });
  fs.writeFileSync(
    path.join(foreign, 'SKILL.md'),
    '---\nname: user-skill\ndescription: Foreign skill owned by the user; trigger never.\n---\n\n# user-skill\n',
  );
  // A retired-name skill carrying our frontmatter must be cleaned.
  const retired = path.join(root, 'engram-learn');
  fs.mkdirSync(retired, { recursive: true });
  fs.writeFileSync(
    path.join(retired, 'SKILL.md'),
    '---\nname: engram-learn\ndescription: Old shipped skill; trigger never.\n---\n\n# engram-learn\n',
  );
  const res = run(pkg, ['pi', '--remove'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  assert.ok(!fs.existsSync(path.join(root, 'engram-index')), 'delivered skill not removed');
  assert.ok(!fs.existsSync(path.join(root, 'engram-factory')), 'delivered skill not removed');
  assert.ok(!fs.existsSync(retired), 'retired-name skill not removed');
  assert.ok(fs.existsSync(path.join(foreign, 'SKILL.md')), 'foreign skill was deleted');
});

// ------------------------------------------------- AC-008 honest limitations
test('AC-008 install --agent all names claude-desktop; capable hosts list paths', () => {
  const { pkg, home, proj } = sandbox('all-limit');
  const res = run(pkg, ['all'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  // Explicit limitation naming the harness and the missing capability.
  assert.match(res.stdout, /note: claude-desktop cannot host skills or subagents/);
  assert.match(res.stdout, /install --client claude-desktop/);
  // Host-capable harnesses still list their written paths.
  assert.ok((res.stdout.match(/^  wrote: /gm) || []).length >= 7, 'capable hosts list written paths');
  for (const label of ['[Claude Code]', '[Codex]', '[GitHub Copilot]', '[pi]']) {
    assert.ok(res.stdout.includes(label), `${label} section present`);
  }
  // Existing per-host notes are preserved (codex /hooks, copilot session-only).
  assert.match(res.stdout, /run \/hooks inside Codex/);
  assert.match(res.stdout, /Copilot command hooks cannot inject per-prompt context/);
});

// --------------------------------------------------- DEC-P04 self-copy guard
test('DEC-P04 same-path guard: in-source project install skips instead of deleting', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-agents-selfcopy-'));
  const pkg = path.join(root, 'pkg');
  fs.cpSync(LAUNCHER, pkg, { recursive: true });
  fs.rmSync(path.join(pkg, 'skills'), { recursive: true, force: true });
  fs.rmSync(path.join(pkg, 'test'), { recursive: true, force: true });
  // Dev fallback resolves <pkg>/../../.agents/skills — the authored-tree
  // position — so the project install below runs with source == destination.
  const authored = path.join(root, '.agents', 'skills');
  fs.mkdirSync(authored, { recursive: true });
  fs.cpSync(path.join(AUTHORED, 'engram-index'), path.join(authored, 'engram-index'), {
    recursive: true,
  });
  const marker = path.join(authored, 'engram-index', 'SURVIVOR-MARKER');
  fs.writeFileSync(marker, 'authored content must survive an in-repo install\n');
  const home = path.join(root, 'home');
  fs.mkdirSync(home);
  const res = run(pkg, ['pi', '--scope', 'project'], { cwd: root, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  assert.ok(fs.existsSync(marker), 'self-copy guard removed authored content');
  assert.ok(fs.existsSync(path.join(authored, 'engram-index', 'SKILL.md')), 'source damaged');
  assert.ok(
    fs.existsSync(path.join(root, '.pi', 'agents', 'red-forman.md')),
    'agent delivery still ran',
  );
});
