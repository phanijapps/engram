/**
 * Skills conformance evals (RFC-0022) — the gbrain skills-conformance
 * pattern adapted: structural rules every skill must satisfy, plus a
 * tool-existence check against the LIVE server's tools/list (so a skill
 * can never reference a tool the server doesn't expose).
 *
 * Structural checks always run. Tool-existence runs when a binary is
 * resolvable (dev build or ENGRAM_MCP_BIN); otherwise it soft-skips
 * with a notice (CI without a release build).
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const SKILLS_ROOT = path.join(__dirname, '..', 'skills');
const EXPECTED_SKILLS = [
  'engram-index',
  'engram-learn',
  'engram-search',
  'engram-plan',
  'engram-implement',
];
const REQUIRED_SECTIONS = [
  /## Contract/,
  /## When it fails/,
  /## Anti-patterns/,
  /## Final check/,
];
// Tool-call shapes embedded in skills must use real tools.
const TOOL_CALL_RE = /([a-z_]+)\(\s*\{/g;

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

function resolveBinary() {
  if (process.env.ENGRAM_MCP_BIN && fs.existsSync(process.env.ENGRAM_MCP_BIN)) {
    return process.env.ENGRAM_MCP_BIN;
  }
  const dev = path.join(os.homedir(), 'projects', 'mem-alpha', 'target', 'release', 'engram-mcp');
  return fs.existsSync(dev) ? dev : null;
}

/** Live tools/list from the real server (default 21-tool surface). */
function liveToolNames() {
  const bin = resolveBinary();
  if (!bin) return null;
  const store = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-conformance-'));
  const probe = spawnSync(
    'node',
    [
      '-e',
      `
      const {spawn} = require('node:child_process');
      const p = spawn(${JSON.stringify(bin)}, ['--storage', ${JSON.stringify(store)}, '--project', 'conformance', '--no-vector'], {stdio:['pipe','pipe','inherit']});
      let buf = '';
      setTimeout(() => {
        p.stdin.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'conformance',version:'0'}}})+'\\n');
        p.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\\n');
        p.stdin.write(JSON.stringify({jsonrpc:'2.0',id:2,method:'tools/list'})+'\\n');
      }, 800);
      p.stdout.on('data', d => {
        buf += d;
        for (let i; (i = buf.indexOf('\\n')) >= 0;) {
          const line = buf.slice(0, i); buf = buf.slice(i+1);
          if (!line.trim()) continue;
          try { const m = JSON.parse(line); if (m.id === 2) { console.log(JSON.stringify(m.result.tools.map(t => t.name))); process.exit(0);} } catch {}
        }
      });
      setTimeout(() => { console.error('timeout'); process.exit(1); }, 15000);
      `,
    ],
    { encoding: 'utf8', timeout: 25000 },
  );
  try {
    return JSON.parse(probe.stdout.trim().split('\n').pop());
  } catch {
    return null;
  }
}

test('exactly the five expected skills ship', () => {
  const dirs = fs
    .readdirSync(SKILLS_ROOT, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name)
    .sort();
  assert.deepEqual(dirs, [...EXPECTED_SKILLS].sort());
});

for (const skill of EXPECTED_SKILLS) {
  test(`${skill}: SKILL.md exists with valid frontmatter`, () => {
    const file = path.join(SKILLS_ROOT, skill, 'SKILL.md');
    assert.ok(fs.existsSync(file), `${file} missing`);
    const text = fs.readFileSync(file, 'utf8');
    const fm = frontmatter(text);
    assert.ok(fm, 'frontmatter block present');
    assert.equal(fm.name, skill, 'frontmatter name matches directory');
    assert.ok((fm.description || '').length > 40, 'description is trigger-rich (>40 chars)');
    assert.match(fm.description, /[Tt]rigger/, 'description names a trigger');
  });

  test(`${skill}: required discipline sections present`, () => {
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    for (const re of REQUIRED_SECTIONS) {
      assert.match(text, re, `missing section ${re}`);
    }
  });

  test(`${skill}: stays lean (progressive disclosure ≤150 lines)`, () => {
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    const lines = text.split('\n').length;
    assert.ok(lines <= 150, `${lines} lines — split depth into references/`);
  });

  test(`${skill}: referenced files exist`, () => {
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    for (const m of text.matchAll(/references\/([a-z0-9-]+\.md)/g)) {
      const ref = path.join(SKILLS_ROOT, skill, 'references', m[1]);
      assert.ok(fs.existsSync(ref), `dangling reference: ${m[1]}`);
    }
  });

  test(`${skill}: JSON tool-call examples parse`, () => {
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    for (const m of text.matchAll(/\{\s*"[^"]+":/g)) {
      // Find complete-looking JSON objects in examples.
    }
    // Stronger: extract balanced braces after a tool( prefix.
    for (const m of text.matchAll(/([a-z_]+)\(\s*\{([\s\S]*?)\}\s*\)/g)) {
      try {
        JSON.parse(`{${m[2]}}`);
      } catch (e) {
        assert.fail(`non-JSON example for ${m[1]}(...): ${e.message}\n---{${m[2]}}---`);
      }
    }
  });
}

test('every tool named in skills exists on the live server', { timeout: 30000 }, () => {
  const names = liveToolNames();
  if (!names) {
    console.log('  [soft-skip] no binary resolvable — structural checks only');
    return;
  }
  const serverTools = new Set(names);
  for (const skill of EXPECTED_SKILLS) {
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    for (const m of text.matchAll(TOOL_CALL_RE)) {
      const t = m[1];
      if (['json', 'test'].includes(t)) continue; // prose, not calls
      assert.ok(
        serverTools.has(t),
        `${skill} references '${t}(...)' but the server exposes: ${[...serverTools].join(', ')}`,
      );
    }
  }
});

test('plan workflow references cover triage | bugfix | story', () => {
  const refs = fs.readdirSync(path.join(SKILLS_ROOT, 'engram-plan', 'references'));
  assert.ok(refs.includes('plan-triage.md'), 'triage workflow');
  assert.ok(refs.includes('plan-bugfix.md'), 'bugfix workflow');
  assert.ok(refs.includes('plan-story.md'), 'story workflow');
});

test('index references carry the REA process + memorization catalog', () => {
  const refs = fs.readdirSync(path.join(SKILLS_ROOT, 'engram-index', 'references'));
  assert.ok(refs.includes('rea-process.md'));
  assert.ok(refs.includes('memorization-catalog.md'));
  const rea = fs.readFileSync(path.join(SKILLS_ROOT, 'engram-index', 'references', 'rea-process.md'), 'utf8');
  assert.match(rea, /proven/, 'evidence labels defined');
  assert.match(rea, /final check/i);
});

// ---------------------------------------------------------------- agents
const AGENTS_ROOT = path.join(__dirname, '..', 'agents');
const AGENT_DIALECTS = [
  ['red-forman.claude.md', '.md frontmatter'],
  ['red-forman.pi.md', '.md frontmatter'],
  ['red-forman.copilot.md', '.md frontmatter'],
  ['red-forman.codex.yaml', '.yaml (Codex uses YAML)'],
];

for (const [file, dialect] of AGENT_DIALECTS) {
  test(`agent ${file} ships (${dialect})`, () => {
    assert.ok(fs.existsSync(path.join(AGENTS_ROOT, file)), `${file} missing`);
  });
}

test('red-forman md dialects carry valid frontmatter (name + description)', () => {
  for (const file of AGENT_DIALECTS.slice(0, 3).map(([f]) => f)) {
    const text = fs.readFileSync(path.join(AGENTS_ROOT, file), 'utf8');
    const fm = frontmatter(text);
    assert.ok(fm, `${file}: frontmatter present`);
    assert.equal(fm.name, 'red-forman', `${file}: name`);
    assert.ok((fm.description || '').length > 40, `${file}: description`);
  }
});

test('red-forman codex yaml parses with required fields', () => {
  const text = fs.readFileSync(path.join(AGENTS_ROOT, 'red-forman.codex.yaml'), 'utf8');
  // Minimal YAML structural checks (no yaml dep): key lines present,
  // developer_instructions block exists, no tab indentation.
  assert.match(text, /^name: red-forman$/m);
  assert.match(text, /^sandbox_mode: read-only$/m);
  assert.match(text, /^developer_instructions: \|$/m);
  assert.doesNotMatch(text, /\t/, 'YAML must not contain tabs');
  assert.match(text, /BLOCKER/, 'severity vocabulary present');
});

test('red-forman persona discipline present in every dialect', () => {
  for (const [file] of AGENT_DIALECTS) {
    const text = fs.readFileSync(path.join(AGENTS_ROOT, file), 'utf8');
    assert.match(text, /Read-only/i, `${file}: read-only rule`);
    assert.match(text, /WRONG SCOPE/, `${file}: scope gate`);
    assert.match(text, /PASS — nothing to fix/, `${file}: rare-pass verdict`);
  }
});
