'use strict';

const fs = require('node:fs');
const path = require('node:path');

// Local skill installations share .agents/skills with authored Engram skills.
// Only Engram's namespace and its outcome-reporting companion are distributed.
function isEngramSkill(name) {
  return /^engram-[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name) || name === 'outcome-reporting';
}

function distributedSkillDirs(root) {
  return fs.readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && isEngramSkill(entry.name)
      && fs.existsSync(path.join(root, entry.name, 'SKILL.md')))
    .map((entry) => entry.name)
    .sort();
}

module.exports = { isEngramSkill, distributedSkillDirs };

if (require.main === module) {
  process.stdout.write(`${JSON.stringify(distributedSkillDirs(process.argv[2]))}\n`);
}
