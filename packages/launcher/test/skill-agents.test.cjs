/**
 * Per-skill agents delivery tests (job 002, DEC-004): each shipped skill's
 * agents/ payload lands as host-dialect files in every host's agents
 * directory, with parseable frontmatter (markdown) or YAML (codex),
 * preserved role names, and contract pointers that resolve from the
 * installed location at BOTH scopes (DEC-P02, REQ-013).
 *
 * Black-box spawns of bin.js with isolated HOME/cwd, like
 * install-agents.test.cjs — nothing real is written.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const LAUNCHER = path.join(__dirname, '..');
const AUTHORED = path.join(LAUNCHER, '..', '..', '.agents', 'skills');
const FACTORY_AGENTS = path.join(AUTHORED, 'engram-factory', 'agents');

// The 14 specialist roles shipped inside the engram-factory skill.
const ROLES = fs
  .readdirSync(FACTORY_AGENTS)
  .map((f) => f.match(/^engram-(.+)\.pi\.md$/))
  .filter(Boolean)
  .map((m) => m[1])
  .sort();
assert.equal(ROLES.length, 14, `expected 14 specialist roles, got ${ROLES.join(', ')}`);

function sandbox(name) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `engram-skill-agents-${name}-`));
  const pkg = path.join(root, 'pkg');
  fs.cpSync(LAUNCHER, pkg, {
    recursive: true,
    filter: (src) =>
      src !== path.join(LAUNCHER, 'skills') && src !== path.join(LAUNCHER, 'test'),
  });
  fs.cpSync(AUTHORED, path.join(pkg, 'skills'), { recursive: true });
  const home = path.join(root, 'home');
  const proj = path.join(root, 'proj');
  fs.mkdirSync(home);
  fs.mkdirSync(proj);
  return { root, pkg, home, proj };
}

function run(pkg, args, { cwd, home }) {
  return spawnSync('node', [path.join(pkg, 'bin.js'), 'install', '--agent', ...args], {
    cwd,
    encoding: 'utf8',
    timeout: 60000,
    env: { ...process.env, HOME: home },
  });
}

function frontmatter(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return null;
  const fm = {};
  for (const line of m[1].split('\n')) {
    const kv = line.match(/^([a-z_]+):\s*(.*)$/);
    if (kv) fm[kv[1]] = kv[2];
  }
  return fm;
}

/** Extract the contract pointer (backticked path ending in references/agents.md). */
function contractPointer(text) {
  const m = text.match(/`([^`]*references\/agents\.md)`/);
  return m ? m[1] : null;
}

/** Resolve a pointer from the installed agent file; returns null for qa (self-contained). */
function assertPointerResolves(wrapperText, resolveFrom, role, label) {
  const ptr = contractPointer(wrapperText);
  if (role === 'qa') {
    assert.equal(ptr, null, `${label}: engram-qa wrapper is self-contained (no pointer)`);
    return;
  }
  assert.ok(ptr, `${label}: contract pointer present`);
  const resolved = ptr.startsWith('.') ? path.join(resolveFrom, ptr) : ptr;
  assert.ok(
    fs.existsSync(resolved),
    `${label}: pointer does not resolve: ${ptr} (from ${resolveFrom})`,
  );
}

// Host -> agents dir + skills root, per scope. `agentFileExt` is the
// installed dialect suffix; codex installs .yaml, the rest .md.
const HOST_LAYOUT = {
  claude: {
    ext: '.md',
    user: (h, p) => ({ agents: path.join(h, '.claude', 'agents'), skills: path.join(h, '.claude', 'skills') }),
    project: (h, p) => ({ agents: path.join(p, '.claude', 'agents'), skills: path.join(p, '.claude', 'skills') }),
  },
  codex: {
    ext: '.yaml',
    user: (h, p) => ({ agents: path.join(h, '.codex', 'agents'), skills: path.join(h, '.agents', 'skills') }),
    project: (h, p) => ({ agents: path.join(h, '.codex', 'agents'), skills: path.join(p, '.agents', 'skills') }),
  },
  copilot: {
    ext: '.md',
    user: (h, p) => ({ agents: path.join(p, '.github', 'agents'), skills: path.join(h, '.agents', 'skills') }),
    project: (h, p) => ({ agents: path.join(p, '.github', 'agents'), skills: path.join(p, '.agents', 'skills') }),
  },
  pi: {
    ext: '.md',
    user: (h, p) => ({ agents: path.join(h, '.pi', 'agent', 'agents'), skills: path.join(h, '.pi', 'agent', 'skills') }),
    project: (h, p) => ({ agents: path.join(p, '.pi', 'agents'), skills: path.join(p, '.agents', 'skills') }),
  },
};

for (const [host, layout] of Object.entries(HOST_LAYOUT)) {
  test(`AC-006 ${host} user scope: 14 dialect files land, parseable, role names preserved`, () => {
    const { pkg, home, proj } = sandbox(host);
    const res = run(pkg, [host], { cwd: proj, home });
    assert.equal(res.status, 0, `stderr: ${res.stderr}`);
    const { agents } = layout.user(home, proj);
    for (const role of ROLES) {
      const file = path.join(agents, `engram-${role}${layout.ext}`);
      assert.ok(fs.existsSync(file), `${file} missing`);
      const text = fs.readFileSync(file, 'utf8');
      if (host === 'codex') {
        assert.match(text, new RegExp(`^name: engram-${role}$`, 'm'), `${file}: name`);
        assert.match(text, /^sandbox_mode: (read-only|workspace-write)$/m, `${file}: sandbox_mode`);
        assert.match(text, /^developer_instructions: \|$/m, `${file}: instructions block`);
        assert.doesNotMatch(text, /\t/, `${file}: YAML must not contain tabs`);
      } else {
        const fm = frontmatter(text);
        assert.ok(fm, `${file}: frontmatter present`);
        assert.equal(fm.name, `engram-${role}`, `${file}: name preserved`);
        assert.ok((fm.description || '').length > 40, `${file}: description`);
      }
    }
    assert.ok(
      fs.existsSync(path.join(agents, `red-forman${layout.ext === '.yaml' ? '.yaml' : '.md'}`)),
      'red-forman still lands',
    );
  });

  test(`AC-007 ${host} user scope: contract pointers resolve from the installed location`, () => {
    const { pkg, home, proj } = sandbox(`${host}-ptr`);
    const res = run(pkg, [host], { cwd: proj, home });
    assert.equal(res.status, 0, `stderr: ${res.stderr}`);
    const { agents } = layout.user(home, proj);
    for (const role of ROLES) {
      const text = fs.readFileSync(path.join(agents, `engram-${role}${layout.ext}`), 'utf8');
      if (role !== 'qa') {
        const ptr = contractPointer(text);
        assert.ok(ptr && path.isAbsolute(ptr), `${host}: ${role} pointer must be absolute at user scope`);
      }
      assertPointerResolves(text, proj, role, `${host} user ${role}`);
    }
  });

  test(`AC-007 ${host} project scope: project-relative pointers resolve from the project`, () => {
    const { pkg, home, proj } = sandbox(`${host}-proj`);
    const res = run(pkg, [host, '--scope', 'project'], { cwd: proj, home });
    assert.equal(res.status, 0, `stderr: ${res.stderr}`);
    const { agents, skills } = layout.project(home, proj);
    // The project-relative skills root the pointer must name for this host.
    const projSkillsRel = host === 'claude' ? '.claude/skills' : '.agents/skills';
    assert.ok(
      fs.existsSync(path.join(proj, projSkillsRel, 'engram-factory', 'references', 'agents.md')),
      `${host}: contract not installed at ${projSkillsRel}`,
    );
    for (const role of ROLES) {
      const text = fs.readFileSync(path.join(agents, `engram-${role}${layout.ext}`), 'utf8');
      if (role !== 'qa') {
        const ptr = contractPointer(text);
        assert.equal(
          ptr,
          `${projSkillsRel}/engram-factory/references/agents.md`,
          `${host}: ${role} pointer must be project-relative and resolve`,
        );
      }
      assertPointerResolves(text, proj, role, `${host} project ${role}`);
    }
  });
}

test('DEC-P06 openai.yaml travels with the skill but never installs as an agent', () => {
  const { pkg, home, proj } = sandbox('openai');
  const res = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  // It rides the skill folder copy...
  assert.ok(
    fs.existsSync(path.join(home, '.pi', 'agent', 'skills', 'engram-factory', 'agents', 'openai.yaml')),
    'openai.yaml travels with the skill folder',
  );
  // ...but no host agents directory receives it.
  for (const dir of [
    path.join(home, '.pi', 'agent', 'agents'),
    path.join(proj, '.pi', 'agents'),
    path.join(proj, '.github', 'agents'),
  ]) {
    assert.ok(!fs.existsSync(path.join(dir, 'openai.yaml')), `${dir}: openai.yaml installed`);
    assert.ok(!fs.existsSync(path.join(dir, 'openai.md')), `${dir}: openai installed`);
  }
});

test('AC-006 sandbox map: builder and qa are workspace-write, the rest read-only', () => {
  const { pkg, home, proj } = sandbox('sandbox-map');
  const res = run(pkg, ['codex'], { cwd: proj, home });
  assert.equal(res.status, 0, `stderr: ${res.stderr}`);
  const dir = path.join(home, '.codex', 'agents');
  for (const role of ROLES) {
    const text = fs.readFileSync(path.join(dir, `engram-${role}.yaml`), 'utf8');
    const expected = ['builder', 'qa'].includes(role) ? 'workspace-write' : 'read-only';
    assert.match(text, new RegExp(`^sandbox_mode: ${expected}$`, 'm'), `${role}: ${expected}`);
  }
});

test('REQ-012 reinstall of agents is idempotent (same bytes, no orphans)', () => {
  const { pkg, home, proj } = sandbox('idem');
  run(pkg, ['pi'], { cwd: proj, home });
  const dir = path.join(home, '.pi', 'agent', 'agents');
  const first = fs.readdirSync(dir).sort();
  const bytes = first.map((f) => fs.readFileSync(path.join(dir, f), 'utf8'));
  run(pkg, ['pi'], { cwd: proj, home });
  assert.deepEqual(fs.readdirSync(dir).sort(), first, 'second install changed the agents set');
  assert.deepEqual(
    first.map((f) => fs.readFileSync(path.join(dir, f), 'utf8')),
    bytes,
    'second install changed agent bytes',
  );
});

test('FIND-code-002 foreign agent file at a delivered name survives install AND --remove', () => {
  const { pkg, home, proj } = sandbox('code002');
  // A user's own agent that merely shares the filename: never installed by
  // engram, different frontmatter name — the exact reviewer reproduction.
  const agentDir = path.join(home, '.pi', 'agent', 'agents');
  fs.mkdirSync(agentDir, { recursive: true });
  const foreign = path.join(agentDir, 'engram-builder.md');
  fs.writeFileSync(
    foreign,
    '---\nname: user-custom-agent\ndescription: The user own agent; trigger never.\n---\n\n# user content\n',
  );
  const install = run(pkg, ['pi'], { cwd: proj, home });
  assert.equal(install.status, 0, `stderr: ${install.stderr}`);
  assert.match(
    fs.readFileSync(foreign, 'utf8'),
    /user-custom-agent/,
    'foreign agent file was clobbered on install',
  );
  assert.match(install.stdout, /exists with different frontmatter — left untouched/);
  // And the same file must survive --remove — the tag-gated removal must
  // spare it while genuinely-ours agent files are cleaned.
  const remove = run(pkg, ['pi', '--remove'], { cwd: proj, home });
  assert.equal(remove.status, 0, `stderr: ${remove.stderr}`);
  assert.ok(fs.existsSync(foreign), 'foreign agent file deleted by --remove');
  assert.ok(!fs.existsSync(path.join(agentDir, 'engram-planner.md')), 'ours not removed');
});

test('FIND-security-004 --remove reports every agent file it removes (audit line)', () => {
  const { pkg, home, proj } = sandbox('sec004');
  const install = run(pkg, ['claude'], { cwd: proj, home });
  assert.equal(install.status, 0, `stderr: ${install.stderr}`);
  const agentDir = path.join(home, '.claude', 'agents');
  const before = fs.readdirSync(agentDir).sort();
  assert.ok(before.length >= 15, `expected >=15 agent files, got ${before.length}`);
  const remove = run(pkg, ['claude', '--remove'], { cwd: proj, home });
  assert.equal(remove.status, 0, `stderr: ${remove.stderr}`);
  assert.deepEqual(fs.readdirSync(agentDir), [], 'agent files not removed');
  // Every removal is named: the audit trail must not understate what the
  // installer deleted from the user home.
  for (const f of before) {
    assert.ok(
      remove.stdout.includes(`${f} (removed)`),
      `no cleaned: audit line for ${f}`,
    );
  }
});
