// Unit seam with explicit stubs, not a ServiceNow runtime or instance test.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('business_rules/validate_quantity.js', 'utf8');
for (const operation of ['insert', 'update']) {
  for (const quantity of ['-1', '0', '2', '', null]) {
    const calls = { abort: [], errors: [], writes: [] };
    const current = {
      getValue: name => { assert.equal(name, 'u_quantity'); return quantity; },
      operation: () => operation,
      setAbortAction: value => calls.abort.push(value),
      setValue: (...args) => calls.writes.push(args),
    };
    const gs = { addErrorMessage: message => calls.errors.push(message) };
    vm.runInNewContext(source, { current, previous: null, gs }, { timeout: 1000 });
    const negative = quantity === '-1';
    assert.deepEqual(calls.abort, negative ? [true] : [], `${operation}/${quantity}: abort`);
    assert.deepEqual(calls.errors, negative ? ['Quantity cannot be negative'] : [], `${operation}/${quantity}: message`);
    assert.deepEqual(calls.writes, [], 'do not rewrite fields');
  }
}
console.log('Local stub checks passed; no ServiceNow instance verification performed');
