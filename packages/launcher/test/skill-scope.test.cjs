'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { checkParity } = require('../scripts/check-skills-parity.cjs');
const { distributedSkillDirs } = require('../scripts/skill-scope.cjs');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-skill-scope-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}
function skill(root, name) {
  const dir = path.join(root, name);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'SKILL.md'), `---\nname: ${name}\n---\n`);
}

test('distribution keeps new Engram skills and outcome reporting, excludes local tools', (t) => {
  const root = fixture(t);
  for (const name of ['engram-future', 'outcome-reporting', 'better-ui', 'archify', 'engramish']) skill(root, name);
  fs.mkdirSync(path.join(root, 'engram-no-contract'));
  assert.deepEqual(distributedSkillDirs(root), ['engram-future', 'outcome-reporting']);
});

test('parity rejects selected top-level links, including dangling links; local links stay local', (t) => {
  const root = fixture(t);
  const source = path.join(root, 'source');
  const payload = path.join(root, 'payload');
  const external = path.join(root, 'external');
  for (const dir of [source, payload, external]) fs.mkdirSync(dir);
  skill(external, 'target');
  fs.symlinkSync(path.join(external, 'target'), path.join(source, 'better-local'));
  const args = { source, payload, piAgents: path.join(root, 'pi') };
  assert.deepEqual(checkParity(args).errors, []);
  fs.symlinkSync(path.join(external, 'target'), path.join(source, 'engram-probe'));
  fs.symlinkSync(path.join(root, 'missing'), path.join(source, 'outcome-reporting'));
  const errors = checkParity(args).errors;
  assert.ok(errors.some((error) => error.includes('symlink (not shippable): engram-probe')));
  assert.ok(errors.some((error) => error.includes('symlink (not shippable): outcome-reporting')));
});

test('distribution CI gate excludes local source but rejects unrelated packaged skills', (t) => {
  const root = fixture(t);
  const repo = path.resolve(__dirname, '../../..');
  const gate = path.join(root, 'tools/ci/check_distribution_drift.py');
  const scripts = path.join(root, 'packages/launcher/scripts');
  fs.mkdirSync(path.dirname(gate), { recursive: true });
  fs.mkdirSync(scripts, { recursive: true });
  fs.copyFileSync(path.join(repo, 'tools/ci/check_distribution_drift.py'), gate);
  fs.copyFileSync(path.join(repo, 'packages/launcher/scripts/skill-scope.cjs'), path.join(scripts, 'skill-scope.cjs'));
  const source = path.join(root, '.agents/skills');
  const payload = path.join(root, 'packages/launcher/skills');
  skill(source, 'engram-probe');
  skill(source, 'better-local');
  skill(payload, 'engram-probe');
  let result = spawnSync('python3', [gate], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  skill(payload, 'better-local');
  result = spawnSync('python3', [gate], { encoding: 'utf8' });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout + result.stderr, /better-local/);
});
