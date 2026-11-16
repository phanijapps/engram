const test = require('node:test');
const assert = require('node:assert/strict');
const { canSave } = require('./form.js');
test('empty and blank names cannot save', () => {
  assert.equal(canSave(''), false);
  assert.equal(canSave('   '), false);
});
test('valid names can save', () => assert.equal(canSave('Ada'), true));
