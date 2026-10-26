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

/** Copy the launcher without its generated payload and tests (parallel
 *  test files may be regenerating the payload via prepack; never race it). */
function copyLauncher(dest) {
  fs.cpSync(LAUNCHER, dest, {
    recursive: true,
    filter: (src) =>
      src !== path.join(LAUNCHER, 'skills') && src !== path.join(LAUNCHER, 'test'),
  });
}

/** A throwaway sandbox: a launcher copy (payload optional) + home + project. */
function sandbox(name, { payload = true } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `engram-agents-${name}-`));
  const pkg = path.join(root, 'pkg');
  copyLauncher(pkg);
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
  copyLauncher(pkg);
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

/** Content hash of every file under `dir` — proves byte-stability of a tree. */
function treeHash(dir) {
  const h = crypto.createHash('sha256');
  for (const rel of tree(dir)) {
    if (!fs.statSync(path.join(dir, rel)).isFile()) continue;
    h.update(rel);
    h.update(fs.readFileSync(path.join(dir, rel)));
  }
  return h.digest('hex');
}

/** Stage `<root>` as the authored repo: launcher copy at
 *  <root>/packages/launcher WITH the generated payload present (what
 *  prepack/tests produce, so skillsSource() picks the payload), authored
 *  tree at <root>/.agents/skills carrying an uncommitted edit, and a stale
 *  byte in the payload copy — the exact FIND-security-001 reproduction at
 *  the real repo layout (launcher two levels below the tree root). */
function authoredRepoStage(name) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `engram-agents-${name}-`));
  const pkg = path.join(root, 'packages', 'launcher');
  copyLauncher(pkg);
  copyAuthored(pkg, AUTHORED_SKILLS); // payload present: skillsSource() picks it
  const authored = path.join(root, '.agents', 'skills');
  fs.mkdirSync(authored, { recursive: true });
  fs.cpSync(AUTHORED, authored, { recursive: true });
  fs.appendFileSync(
    path.join(authored, 'engram-index', 'SKILL.md'),
    '\n<!-- UNCOMMITTED EDIT -->\n',
  );
  fs.writeFileSync(
    path.join(authored, 'engram-index', 'UNCOMMITTED.md'),
    'uncommitted authoring work must survive\n',
  );
  fs.appendFileSync(
    path.join(pkg, 'skills', 'engram-index', 'SKILL.md'),
    '\n<!-- stale payload byte -->\n',
  );
  const home = path.join(root, 'home');
  fs.mkdirSync(home);
  return { root, pkg, home, authored };
}

test('FIND-security-001 payload-present project install never touches the authored tree', () => {
  const { root, pkg, home, authored } = authoredRepoStage('sec001');
  const before = treeHash(authored);
  const res = run(pkg, ['pi', '--scope', 'project'], { cwd: root, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  // The guard fired (loudly) instead of copying...
  assert.match(res.stdout, /is the engram skills source — left untouched/);
  // ...and the authored tree is byte-identical: the uncommitted edit lives,
  // the stale payload byte never landed, nothing was deleted.
  assert.equal(treeHash(authored), before, 'authored tree changed by install');
  assert.ok(
    fs.readFileSync(path.join(authored, 'engram-index', 'SKILL.md'), 'utf8').includes(
      'UNCOMMITTED EDIT',
    ),
    'uncommitted authored edit destroyed',
  );
  assert.doesNotMatch(
    fs.readFileSync(path.join(authored, 'engram-index', 'SKILL.md'), 'utf8'),
    /stale payload byte/,
    'authored tree was overwritten with (stale) payload bytes',
  );
  assert.ok(
    fs.existsSync(path.join(root, '.pi', 'agents', 'red-forman.md')),
    'agent delivery still ran',
  );
});

test('FIND-operations-003 a failing host exits non-zero while remaining hosts complete', () => {
  const { pkg, home, proj } = sandbox('partial');
  // Poison one host: ~/.claude as a file makes its mkdir fail (ENOTDIR).
  fs.writeFileSync(path.join(home, '.claude'), 'not a directory\n');
  const res = run(pkg, ['all'], { cwd: proj, home });
  assert.notEqual(res.status, 0, 'partial install must not exit 0');
  assert.match(res.stderr, /ENOTDIR/);
  assert.match(res.stdout, /Incomplete: 1 host\(s\) failed/);
  // Fail-open continuation: every other host section completed its writes.
  for (const label of ['[Codex]', '[GitHub Copilot]', '[pi]']) {
    assert.ok(res.stdout.includes(label), `${label} section missing`);
  }
  assert.ok(
    fs.existsSync(path.join(home, '.pi', 'agent', 'skills', 'engram-index', 'SKILL.md')),
    'pi host did not complete after claude failed',
  );
  assert.ok(
    fs.existsSync(path.join(home, '.agents', 'skills', 'engram-index', 'SKILL.md')),
    'codex/copilot host did not complete after claude failed',
  );
  // The healthy-path exit code is unchanged (0) — sanity against collateral.
  const { pkg: pkg2, home: home2, proj: proj2 } = sandbox('partial-ok');
  const ok = run(pkg2, ['all'], { cwd: proj2, home: home2 });
  assert.equal(ok.status, 0, `healthy run must stay exit 0: ${ok.stderr}`);
});

test('FIND-code-004 dev-fallback copy applies the hygiene filter (__pycache__/*.pyc never ship)', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-agents-hygiene-'));
  const pkg = path.join(root, 'packages', 'launcher');
  copyLauncher(pkg); // no payload: skillsSource() resolves the dev fallback
  const authored = path.join(root, '.agents', 'skills');
  fs.mkdirSync(authored, { recursive: true });
  fs.cpSync(path.join(AUTHORED, 'engram-index'), path.join(authored, 'engram-index'), {
    recursive: true,
  });
  fs.cpSync(path.join(AUTHORED, 'engram-factory'), path.join(authored, 'engram-factory'), {
    recursive: true,
  });
  // A dev checkout's local Python debris under the source tree.
  fs.mkdirSync(path.join(authored, 'engram-factory', 'scripts', '__pycache__'), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(authored, 'engram-factory', 'scripts', '__pycache__', 'factory.cpython-311.pyc'),
    'junk',
  );
  fs.writeFileSync(path.join(authored, 'engram-factory', 'scripts', 'junk.pyc'), 'junk');
  const home = path.join(root, 'home');
  fs.mkdirSync(home);
  const proj = path.join(root, 'proj');
  fs.mkdirSync(proj);
  const res = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  const delivered = path.join(home, '.pi', 'agent', 'skills');
  assert.ok(fs.existsSync(path.join(delivered, 'engram-index', 'SKILL.md')), 'skills missing');
  assert.ok(
    !fs.existsSync(path.join(delivered, 'engram-factory', 'scripts', '__pycache__')),
    '__pycache__ shipped',
  );
  assert.ok(!fs.existsSync(path.join(delivered, 'engram-factory', 'scripts', 'junk.pyc')), '*.pyc shipped');
});

test('FIND-security-002 foreign skill content at a delivered path survives install; our-tagged dirs are replaced', () => {
  const { pkg, home, proj } = sandbox('sec002');
  const res0 = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res0.status, 0, `stderr: ${res0.stderr}`);
  const root = path.join(home, '.pi', 'agent', 'skills');
  // Foreign replacement in our namespace: different frontmatter name tag.
  const foreign = path.join(root, 'engram-index');
  fs.rmSync(foreign, { recursive: true, force: true });
  fs.mkdirSync(foreign, { recursive: true });
  fs.writeFileSync(
    path.join(foreign, 'SKILL.md'),
    '---\nname: my-own-engram-index\ndescription: User-authored replacement; trigger never.\n---\n\n# mine\n',
  );
  fs.writeFileSync(path.join(foreign, 'USER-MARKER'), 'user content\n');
  // Our own delivered copy with a local extra file: tag matches ours, so
  // replacement must keep working (REQ-012 idempotency).
  fs.writeFileSync(path.join(root, 'engram-factory', 'LOCAL-NOTES.md'), 'local edit\n');
  const res = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  assert.ok(
    fs.readFileSync(path.join(foreign, 'SKILL.md'), 'utf8').includes('my-own-engram-index'),
    'foreign skill dir was destroyed by install',
  );
  assert.ok(fs.existsSync(path.join(foreign, 'USER-MARKER')), 'foreign content deleted');
  assert.match(res.stdout, /exists with different frontmatter — left untouched/);
  assert.match(
    fs.readFileSync(path.join(root, 'engram-factory', 'SKILL.md'), 'utf8'),
    /^---\nname: engram-factory\n/m,
    'our-tagged dir was not refreshed',
  );
  assert.ok(
    !fs.existsSync(path.join(root, 'engram-factory', 'LOCAL-NOTES.md')),
    'our-tagged dir must be fully replaced (REQ-012), extra local file survived',
  );
});

test('FIND-code-001 in-repo --remove never deletes the authored tree', () => {
  const { root, pkg, home, authored } = authoredRepoStage('code001');
  const before = treeHash(authored);
  const res = run(pkg, ['pi', '--scope', 'project', '--remove'], { cwd: root, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  assert.equal(treeHash(authored), before, '--remove deleted authored content');
  assert.ok(
    fs.existsSync(path.join(authored, 'engram-index', 'UNCOMMITTED.md')),
    'uncommitted authoring work destroyed by --remove',
  );
  assert.match(res.stdout, /is the engram skills source — left untouched/);
});

// ------------------------------------------- FIND-security-005 (round-2 review)
// The guard's candidate list is location-derived, so a dest root symlinked
// into ANOTHER checkout's skills tree resolves outside every candidate —
// these tests reproduce the reviewer's two bypass probes (project `.claude`
// link; user `~/.agents/skills` link) and must fail loudly, never write.

/** A SECOND checkout `<B>` carrying a full authored skills tree with an
 *  uncommitted edit — the tree a linked dest root would silently destroy. */
function foreignCheckout(name) {
  const b = fs.mkdtempSync(path.join(os.tmpdir(), `engram-agents-${name}-`));
  const authored = path.join(b, '.agents', 'skills');
  fs.mkdirSync(authored, { recursive: true });
  fs.cpSync(AUTHORED, authored, { recursive: true });
  fs.appendFileSync(
    path.join(authored, 'engram-index', 'SKILL.md'),
    '\n<!-- FOREIGN UNCOMMITTED EDIT -->\n',
  );
  return { b, authored };
}

test('FIND-security-005 repro 1: project .claude symlinked into a foreign tree — refused, tree survives', () => {
  const { root, pkg, home } = authoredRepoStage('sec005a'); // checkout <A> at <root>
  const { b, authored } = foreignCheckout('sec005b'); // a DIFFERENT checkout <B>
  fs.symlinkSync(path.join(b, '.agents'), path.join(root, '.claude'));
  const before = treeHash(authored);
  const res = run(pkg, ['claude', '--scope', 'project'], { cwd: root, home });
  assert.notEqual(res.status, 0, 'install through a symlinked dest root must not exit 0');
  assert.match(res.stderr, /refusing to write through a symlinked destination/);
  assert.match(res.stderr, /\.claude/);
  assert.equal(treeHash(authored), before, 'foreign authored tree modified through the link');
  assert.ok(
    !fs
      .readFileSync(path.join(authored, 'engram-index', 'SKILL.md'), 'utf8')
      .includes('stale payload byte'),
    'payload bytes landed in the foreign tree through the link',
  );
  // --remove must refuse through the same link, not delete the foreign tree.
  const rem = run(pkg, ['claude', '--scope', 'project', '--remove'], { cwd: root, home });
  assert.notEqual(rem.status, 0, '--remove through a symlinked dest root must not exit 0');
  assert.match(rem.stderr, /refusing to write through a symlinked destination/);
  assert.equal(treeHash(authored), before, '--remove deleted foreign content through the link');
});

test('FIND-security-005 repro 2: ~/.agents/skills symlinked into a foreign tree — refused before any write', () => {
  const { root, pkg, home } = authoredRepoStage('sec005c'); // launcher at <A>
  const { b, authored } = foreignCheckout('sec005d');
  fs.mkdirSync(path.join(home, '.agents'));
  fs.symlinkSync(path.join(b, '.agents', 'skills'), path.join(home, '.agents', 'skills'));
  const before = treeHash(authored);
  const res = run(pkg, ['codex'], { cwd: root, home });
  assert.notEqual(res.status, 0, 'user-scope install through a linked skills root must not exit 0');
  assert.match(res.stderr, /refusing to write through a symlinked destination/);
  assert.equal(treeHash(authored), before, 'foreign authored tree modified through the link');
  // Fail-closed within the host: the skills copy is first, so the refusal
  // fires before agents/config writes — nothing else lands either.
  assert.ok(!fs.existsSync(path.join(home, '.codex')), 'host continued writing after the refusal');
});

test('FIND-security-005 agents and settings dest roots carry the same refusal', () => {
  const { pkg, home, proj } = sandbox('sec005e');
  const { b, authored } = foreignCheckout('sec005f');
  fs.mkdirSync(path.join(home, '.claude')); // skills dest is real…
  fs.symlinkSync(path.join(b, '.agents'), path.join(home, '.claude', 'agents')); // …agents dest is a link
  const before = treeHash(authored);
  const res = run(pkg, ['claude'], { cwd: proj, home });
  assert.notEqual(res.status, 0, 'install through a symlinked agents dir must not exit 0');
  assert.match(res.stderr, /refusing to write through a symlinked destination/);
  // Skills landed (their dest is a real dir)…
  assert.ok(
    fs.existsSync(path.join(home, '.claude', 'skills', 'engram-index', 'SKILL.md')),
    'skills delivery broken by the agents-dir refusal',
  );
  // …but no agent file was written through the link and settings never landed.
  assert.ok(!fs.existsSync(path.join(b, '.agents', 'red-forman.md')), 'agent written through the link');
  assert.ok(
    !fs.existsSync(path.join(b, '.agents', 'engram-builder.md')),
    'dialect agent written through the link',
  );
  assert.ok(
    !fs.existsSync(path.join(home, '.claude', 'settings.json')),
    'settings written after the host should have aborted',
  );
  assert.equal(treeHash(authored), before, 'foreign tree modified through the agents link');
});

test('FIND-security-007 ≡ FIND-code-006 dev-fallback copy never delivers symlinks from skill bodies', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-agents-bodylink-'));
  const pkg = path.join(root, 'packages', 'launcher');
  copyLauncher(pkg); // no payload: the authored dev fallback is the source
  const authored = path.join(root, '.agents', 'skills');
  fs.mkdirSync(authored, { recursive: true });
  fs.cpSync(AUTHORED, authored, { recursive: true });
  // A link planted in a dirty dev tree — the reviewer's /etc/passwd probe.
  fs.symlinkSync('/etc/passwd', path.join(authored, 'engram-index', 'dev-link'));
  const home = path.join(root, 'home');
  fs.mkdirSync(home);
  const res = run(pkg, ['pi'], { cwd: root, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  const delivered = path.join(home, '.pi', 'agent', 'skills', 'engram-index');
  assert.ok(fs.existsSync(path.join(delivered, 'SKILL.md')), 'skills missing');
  assert.ok(!fs.existsSync(path.join(delivered, 'dev-link')), 'a live symlink shipped into the user skills dir');
  assert.ok(
    !fs.readdirSync(path.join(home, '.pi', 'agent', 'skills', 'engram-index')).includes('dev-link'),
    'link entry present under any spelling',
  );
  assert.match(res.stdout, /dev-link is a symlink — not shipped/, 'refusal must be loud');
});
