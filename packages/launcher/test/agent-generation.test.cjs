'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { checkParity } = require('../scripts/check-skills-parity.cjs');
const LAUNCHER = path.resolve(__dirname, '..');
const AUTHORED = path.resolve(LAUNCHER, '../../.agents/skills');

function command(bin, args, options = {}) {
  const result = spawnSync(bin, args, { encoding: 'utf8', timeout: 60000, ...options });
  assert.equal(result.status, 0, `${bin} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-generation-'));
  const pkg = path.join(root, 'packages/launcher');
  fs.cpSync(LAUNCHER, pkg, { recursive: true, filter: (src) => !['skills', 'test'].some((part) => src === path.join(LAUNCHER, part)) });
  const source = path.join(root, '.agents/skills');
  fs.cpSync(path.join(AUTHORED, 'engram-factory'), path.join(source, 'engram-factory'), { recursive: true });
  fs.mkdirSync(path.join(source, 'engram-index'), { recursive: true });
  fs.writeFileSync(path.join(source, 'engram-index/SKILL.md'), '---\nname: engram-index\n---\n');
  const home = path.join(root, 'home'), proj = path.join(root, 'project');
  fs.mkdirSync(home); fs.mkdirSync(proj);
  const prepack = () => command('node', [path.join(pkg, 'scripts/generate-skills-payload.cjs')]);
  const install = () => command('node', [path.join(pkg, 'bin.js'), 'install', '--agent', 'pi'], { cwd: proj, env: { ...process.env, HOME: home } });
  return { root, pkg, source, home, proj, prepack, install };
}
function treeBytes(root) {
  const out = {};
  function visit(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isSymbolicLink()) out[path.relative(root, file)] = { symlink: fs.readlinkSync(file) };
      else if (entry.isDirectory()) visit(file);
      else out[path.relative(root, file)] = fs.readFileSync(file).toString('base64');
    }
  }
  visit(root); return out;
}

test('generic parity fixtures without a declaration skip canonical verification honestly', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-generic-parity-'));
  const source = path.join(root, '.agents/skills');
  fs.mkdirSync(path.join(source, 'fixture'), { recursive: true });
  fs.writeFileSync(path.join(source, 'fixture/SKILL.md'), 'fixture\n');
  const result = checkParity({ source, payload: path.join(root, 'absent'), piAgents: path.join(root, '.pi/agents') });
  assert.deepEqual(result.errors, []);
  assert.equal(result.canonicalDeclared, false);
  assert.equal(result.payloadPresent, false);
});

test('canonical drift fails parity even when authored, payload, and Pi wrappers agree; check is read-only', () => {
  const f = fixture(); f.prepack();
  const roots = { source: f.source, payload: path.join(f.pkg, 'skills'), piAgents: path.join(f.root, '.pi/agents') };
  assert.deepEqual(checkParity(roots).errors, []);
  const canonical = path.join(f.source, 'engram-factory/references/roles/builder.md');
  fs.appendFileSync(canonical, '\nFresh fixture contract requirement.\n');
  // Keep delivery bytes equal to authored: only canonical derivation can catch stale wrappers.
  fs.copyFileSync(canonical, path.join(roots.payload, 'engram-factory/references/roles/builder.md'));
  const before = treeBytes(f.root);
  const result = checkParity(roots);
  assert.equal(result.canonicalDeclared, true);
  assert.ok(result.errors.some((e) => e.includes('canonical agents generation check failed')), result.errors.join('\n'));
  assert.deepEqual(treeBytes(f.root), before, 'read-only parity modified fixture files');
});

test('prepack refreshes canonical wrappers and reinstall updates managed instructions while foreign files survive', () => {
  const f = fixture(); f.prepack(); f.install();
  const agents = path.join(f.home, '.pi/agent/agents');
  const builder = path.join(agents, 'engram-builder.md');
  const old = fs.readFileSync(builder, 'utf8');
  const digest = (s) => s.match(/bundle-sha256:\s*([a-f0-9]{64})/)[1];
  const oldDigest = digest(old);
  fs.appendFileSync(builder, '\nLocal edit to managed generated file.\n');
  const foreign = path.join(agents, 'engram-planner.md');
  const foreignBytes = '---\nname: my-own-planner\ndescription: Foreign fixture.\n---\nCustom instructions.\n';
  fs.writeFileSync(foreign, foreignBytes);
  const canonical = path.join(f.source, 'engram-factory/references/roles/builder.md');
  fs.appendFileSync(canonical, '\nRequire fresh canonical fixture proof before handoff.\n');
  f.prepack(); f.install();
  const installed = fs.readFileSync(builder, 'utf8');
  assert.notEqual(digest(installed), oldDigest, 'installed provenance remained stale');
  const generated = fs.readFileSync(path.join(f.source, 'engram-factory/agents/engram-builder.pi.md'), 'utf8');
  assert.equal(installed, generated.split('.agents/skills/engram-factory/').join(`${path.join(f.home, '.pi/agent/skills/engram-factory')}/`));
  assert.equal(fs.readFileSync(foreign, 'utf8'), foreignBytes, 'foreign name-tagged file was overwritten');
  const installedContract = path.join(f.home, '.pi/agent/skills/engram-factory/references/roles/builder.md');
  assert.equal(fs.readFileSync(installedContract, 'utf8'), fs.readFileSync(canonical, 'utf8'));
  assert.match(fs.readFileSync(installedContract, 'utf8'), /Require fresh canonical fixture proof before handoff/);
  assert.deepEqual(checkParity({ source: f.source, payload: path.join(f.pkg, 'skills'), piAgents: path.join(f.root, '.pi/agents') }).errors, []);
});


test('declared generator failure aborts prepack before replacing the existing payload', () => {
  const f = fixture(); f.prepack();
  const before = treeBytes(path.join(f.pkg, 'skills'));
  fs.unlinkSync(path.join(f.source, 'engram-factory/scripts/generate_agents.py'));
  const result = spawnSync('node', [path.join(f.pkg, 'scripts/generate-skills-payload.cjs')], { encoding: 'utf8', timeout: 60000 });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /canonical agents generation failed/);
  assert.deepEqual(treeBytes(path.join(f.pkg, 'skills')), before);
});


test('prepack rejects a canonical source edit between generation and payload copy', () => {
  const f = fixture(); f.prepack();
  const hook = path.join(f.root, 'copy-race.cjs');
  const source = path.join(f.source, 'engram-factory');
  const canonical = path.join(source, 'references/roles/builder.md');
  fs.writeFileSync(hook, `const fs = require('node:fs');
const original = fs.cpSync;
fs.cpSync = function (source, dest, options) {
  if (source === ${JSON.stringify(source)}) fs.appendFileSync(${JSON.stringify(canonical)}, '\\nConcurrent canonical fixture edit.\\n');
  return original.call(this, source, dest, options);
};
`);
  const result = spawnSync('node', ['--require', hook, path.join(f.pkg, 'scripts/generate-skills-payload.cjs')], { encoding: 'utf8', timeout: 60000 });
  assert.notEqual(result.status, 0, 'mixed canonical candidate reported success');
  assert.match(result.stderr, /(?:source|copied) canonical agents check failed/);
});


test('prepack rejects a canonical source edit after its files were already copied', () => {
  const f = fixture(); f.prepack();
  const hook = path.join(f.root, 'after-copy-race.cjs');
  const source = path.join(f.source, 'engram-factory');
  const canonical = path.join(source, 'references/shared.md');
  fs.writeFileSync(hook, `const fs = require('node:fs');
const original = fs.cpSync;
fs.cpSync = function (source, dest, options) {
  const result = original.call(this, source, dest, options);
  if (source === ${JSON.stringify(source)}) fs.appendFileSync(${JSON.stringify(canonical)}, '\\nCanonical edit after copy.\\n');
  return result;
};
`);
  const result = spawnSync('node', ['--require', hook, path.join(f.pkg, 'scripts/generate-skills-payload.cjs')], { encoding: 'utf8', timeout: 60000 });
  assert.notEqual(result.status, 0, 'self-consistent old payload reported fresh-source success');
  assert.match(result.stderr, /source canonical agents check failed/);
});
