/**
 * Skills conformance evals (RFC-0022) — the gbrain skills-conformance
 * pattern adapted: structural rules skills must satisfy, plus a
 * tool-existence check against the LIVE server's tools/list (so a skill
 * can never reference a tool the server doesn't expose).
 *
 * Job 002 (DEC-002/DEC-P05): the expected set is DERIVED from the authored
 * tree (.agents/skills — the single source), never a second hardcoded list,
 * and structural rules split by skill class. Knowledge skills keep every
 * historical assertion verbatim (discipline sections, ≤150-line lean,
 * trigger-word description, the four bespoke behavior tests). Workflow
 * skills (engram-factory, engram-frame, outcome-reporting) are checked by
 * their class: frontmatter, referenced files, JSON tool-call examples, and
 * live tool existence. The suite also pins the factory contract's
 * recall/memory guidance (AC-011) and that no skill-listing MCP tool exists
 * (REQ-009 — the installer is the only exposure surface this phase).
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

const SKILLS_ROOT = path.join(__dirname, '..', '..', '..', '.agents', 'skills');
// Skill classes (DEC-P05): knowledge skills keep the full structural rule
// set; workflow skills are checked by their (looser) class rules.
const KNOWLEDGE_SKILLS = [
  'engram-distill',
  'engram-index',
  'engram-know',
  'engram-mem',
];
// Derived from the authored source — the same derivation the installer and
// the payload generator use. No third list lives here (DEC-002).
const AUTHORED_SKILLS = fs
  .readdirSync(SKILLS_ROOT, { withFileTypes: true })
  .filter((d) => d.isDirectory() && fs.existsSync(path.join(SKILLS_ROOT, d.name, 'SKILL.md')))
  .map((d) => d.name)
  .sort();
const WORKFLOW_SKILLS = AUTHORED_SKILLS.filter((s) => !KNOWLEDGE_SKILLS.includes(s)).sort();
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

/** Live tools/list from the real server, launched with extra args (tool profile). */
function liveToolNames(extraArgs = []) {
  const bin = resolveBinary();
  if (!bin) return null;
  const store = fs.mkdtempSync(path.join(os.tmpdir(), 'engram-conformance-'));
  const probe = spawnSync(
    'node',
    [
      '-e',
      `
      const {spawn} = require('node:child_process');
      const p = spawn(${JSON.stringify(bin)}, ['--storage', ${JSON.stringify(store)}, '--project', 'conformance', '--no-vector', ...${JSON.stringify(extraArgs)}], {stdio:['pipe','pipe','inherit']});
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

test('expected set derives from the authored source, knowledge skills all ship', () => {
  assert.deepEqual(
    AUTHORED_SKILLS,
    fs
      .readdirSync(SKILLS_ROOT, { withFileTypes: true })
      .filter((d) => d.isDirectory() && fs.existsSync(path.join(SKILLS_ROOT, d.name, 'SKILL.md')))
      .map((d) => d.name)
      .sort(),
    'derived expectation must match the source scan',
  );
  for (const skill of KNOWLEDGE_SKILLS) {
    assert.ok(AUTHORED_SKILLS.includes(skill), `knowledge skill ${skill} missing from the authored set`);
  }
  // The workflow class is exactly the authored set minus knowledge — today
  // engram-factory, engram-frame, outcome-reporting (DEC-005).
  assert.deepEqual(WORKFLOW_SKILLS, ['engram-factory', 'engram-frame', 'outcome-reporting'].sort());
});

for (const skill of KNOWLEDGE_SKILLS) {
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

// Workflow class (DEC-P05): frontmatter, referenced files, JSON examples —
// no discipline-section or ≤150-line rules (workflow skills carry their
// process depth in references/ by design; EVD-014).
for (const skill of WORKFLOW_SKILLS) {
  test(`${skill}: SKILL.md exists with valid frontmatter (workflow class)`, () => {
    const file = path.join(SKILLS_ROOT, skill, 'SKILL.md');
    assert.ok(fs.existsSync(file), `${file} missing`);
    const text = fs.readFileSync(file, 'utf8');
    const fm = frontmatter(text);
    assert.ok(fm, 'frontmatter block present');
    assert.equal(fm.name, skill, 'frontmatter name matches directory');
    assert.ok((fm.description || '').length > 40, 'description is substantive (>40 chars)');
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
    for (const m of text.matchAll(/([a-z_]+)\(\s*\{([\s\S]*?)\}\s*\)/g)) {
      // Workflow skills document unions like "success": true|false — pick the
      // first alternative, then require strict JSON structure (balanced
      // braces, quoted keys, valid values). Knowledge skills keep the
      // verbatim strict check above.
      const normalized = m[2].replace(/(\btrue|\bfalse|"[^"]*")\|(\btrue|\bfalse|"[^"]*")/g, '$1');
      try {
        JSON.parse(`{${normalized}}`);
      } catch (e) {
        assert.fail(`non-JSON example for ${m[1]}(...): ${e.message}\n---{${m[2]}}---`);
      }
    }
  });
}

test('every tool named in skills exists on the live server', { timeout: 30000 }, () => {
  const bin = resolveBinary();
  if (!bin) {
    console.log('  [soft-skip] no binary resolvable — structural checks only');
    return;
  }
  // Each skill is validated against the tool profile it documents.
  // Default installs run the core profile; distill requires --tools full.
  const PROFILE_FOR_SKILL = {
    'engram-index': [],
    'engram-know': [],
    'engram-mem': [],
    'engram-distill': ['--tools', 'all'],
    // Workflow skills document the core profile (factory/frame carry no
    // tool-call examples; outcome-reporting uses report_outcome).
    'engram-factory': [],
    'engram-frame': [],
    'outcome-reporting': [],
  };
  const surfaces = new Map(); // JSON key of args -> Set of tool names
  for (const args of new Set(Object.values(PROFILE_FOR_SKILL).map((a) => JSON.stringify(a)))) {
    const names = liveToolNames(JSON.parse(args));
    assert.ok(names, `tools/list failed for profile ${args}`);
    surfaces.set(args, new Set(names));
  }
  for (const skill of AUTHORED_SKILLS) {
    const serverTools = surfaces.get(JSON.stringify(PROFILE_FOR_SKILL[skill]));
    const text = fs.readFileSync(path.join(SKILLS_ROOT, skill, 'SKILL.md'), 'utf8');
    for (const m of text.matchAll(TOOL_CALL_RE)) {
      const t = m[1];
      if (['json', 'test'].includes(t)) continue; // prose, not calls
      assert.ok(
        serverTools.has(t),
        `${skill} references '${t}(...)' but its profile exposes: ${[...serverTools].join(', ')}`,
      );
    }
  }
  // REQ-009: the installer is the only exposure surface this phase — the
  // live tool set must not contain a skill-listing tool.
  const core = surfaces.get(JSON.stringify([]));
  const skillTools = [...core].filter((t) => /skill/i.test(t));
  assert.deepEqual(skillTools, [], 'skill-listing MCP tools must not exist this phase');
});

test('know is read-only and hands records off', () => {
  const text = fs.readFileSync(path.join(SKILLS_ROOT, 'engram-know', 'SKILL.md'), 'utf8');
  for (const writeTool of ['write_memory', 'store_knowledge', 'put_entity', 'put_relationship', 'index_docs', 'scan_repo']) {
    assert.doesNotMatch(text, new RegExp(`${writeTool}\\s*\\(`), `know must not call ${writeTool}`);
  }
  assert.match(text, /engram-mem/, 'know hands experience records to engram-mem');
  assert.match(text, /engram-distill/, 'know hands source-grounded records to engram-distill');
});

test('mem owns experience writes and routes source facts out', () => {
  const text = fs.readFileSync(path.join(SKILLS_ROOT, 'engram-mem', 'SKILL.md'), 'utf8');
  assert.match(text, /write_memory\(\s*\{/, 'mem writes memories');
  assert.match(text, /procedure_put\(\s*\{/, 'mem writes procedures');
  assert.doesNotMatch(text, /store_knowledge\s*\(/, 'mem must not call store_knowledge');
  assert.match(text, /engram-distill/, 'mem routes source-doc facts to engram-distill');
});

test('distill keys writes by source and is honest about supersession scope', () => {
  const text = fs.readFileSync(path.join(SKILLS_ROOT, 'engram-distill', 'SKILL.md'), 'utf8');
  assert.match(text, /<[a-z-]*id>@/, 'idempotency key names the source');
  assert.match(text, /sha256-of-exact-material|content-hash/, 'key binds exact source bytes');
  assert.match(text, /source_id/, 'source_id discipline stated');
  assert.match(text, /same[^\n]*source_id|source_id[^\n]*same/, 'revisions reuse the same source_id');
  assert.match(text, /supersed/, 'revision supersession rule present');
  const appendOnlyLines = text.split('\n').filter((line) => /append-only/i.test(line));
  assert.ok(appendOnlyLines.length > 0, 'append-only limitation stated');
  for (const line of appendOnlyLines) {
    assert.match(
      line,
      /source_id|unsourced|omitted/i,
      `append-only must be scoped to batches without source_id: ${line.trim()}`
    );
  }
});

test('index references carry the REA process + memorization catalog', () => {
  const refs = fs.readdirSync(path.join(SKILLS_ROOT, 'engram-index', 'references'));
  assert.ok(refs.includes('rea-process.md'));
  assert.ok(refs.includes('memorization-catalog.md'));
  const rea = fs.readFileSync(path.join(SKILLS_ROOT, 'engram-index', 'references', 'rea-process.md'), 'utf8');
  assert.match(rea, /proven/, 'evidence labels defined');
  assert.match(rea, /final check/i);
});

// ------------------------------------------------- factory skill payload (job 002)
test('AC-011 factory contract: every role section instructs engram recall/memory', () => {
  const contract = fs.readFileSync(
    path.join(SKILLS_ROOT, 'engram-factory', 'references', 'agents.md'),
    'utf8',
  );
  const sections = contract.split('\n### ').slice(1);
  const required = [
    'engram-analyzer',
    'engram-planner',
    'engram-tasks',
    'engram-builder',
    'Specialist reviewers',
    'engram-qa',
    'engram-approver',
  ];
  const names = sections.map((s) => s.split('\n')[0]);
  for (const r of required) {
    assert.ok(names.includes(r), `contract section '${r}' missing`);
  }
  for (const section of sections) {
    assert.match(
      section,
      /recall|memory/,
      `section '${section.split('\n')[0]}' lacks engram recall/memory guidance`,
    );
  }
});

test('DEC-P06 factory skill ships the four-dialect specialist payload', () => {
  const agentsDir = path.join(SKILLS_ROOT, 'engram-factory', 'agents');
  assert.ok(fs.existsSync(agentsDir), 'agents payload missing from the factory skill');
  const files = fs.readdirSync(agentsDir);
  const dialects = files.filter(
    (f) => /^engram-.+\.(claude|pi|copilot)\.md$/.test(f) || /^engram-.+\.codex\.yaml$/.test(f),
  );
  assert.equal(dialects.length, 56, `expected 56 dialect files, found ${dialects.length}`);
  // The interface manifest travels with the folder but is not a dialect.
  assert.ok(files.includes('openai.yaml'), 'openai.yaml travels with the skill folder');
  assert.ok(!dialects.includes('openai.yaml'));
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
