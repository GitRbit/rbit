import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';
const source = await readFile(new URL('../../crates/rbit-web/assets/repo-create.js', import.meta.url), 'utf8');
const context = vm.createContext({ document: { addEventListener() {} } });
vm.runInContext(source, context);
function panel(fields) {
  return { querySelectorAll(selector) { return fields.filter(field => selector === `[name="${field.name}"]`); } };
}
test('switching types carries shared fields while preserving each form draft', () => {
  const common = {};
  const first = panel([{ name: 'owner', value: 'org' }, { name: 'name', value: 'project' }, { name: 'description', value: 'draft' }, { name: 'visibility', type: 'radio', checked: true, value: 'private' }]);
  const fields = [{ name: 'owner', tagName: 'SELECT', options: [{value: 'org'}, {value: 'me'}], value: 'me' }, {name: 'name', value: ''}, {name: 'url', value: 'https://example.com/repo.git'}, {name: 'visibility', type: 'radio', value: 'private', checked: false}, {name: 'visibility', type: 'radio', value: 'public', checked: true}];
  context.first = first; context.next = panel(fields); context.common = common;
  vm.runInContext('readCommon(first, common); writeCommon(next, common)', context);
  assert.equal(fields[0].value, 'org'); assert.equal(fields[1].value, 'project');
  assert.equal(fields[2].value, 'https://example.com/repo.git');
  assert.equal(fields[3].checked, true); assert.equal(fields[4].checked, false);
  vm.runInContext('readCommon(next, common)', context);
  assert.equal(common.description, 'draft', 'a form without description must keep the shared draft');
});
test('copying an owner cannot select an unavailable organization', () => {
  const owner = { name: 'owner', tagName: 'SELECT', options: [{ value: 'me' }], value: 'me' };
  context.next = panel([owner]); context.common = { owner: 'unavailable' };
  vm.runInContext('writeCommon(next, common)', context);
  assert.equal(owner.value, 'me');
});

test('returning to the active tab clears a cancelled form load status', async () => {
  const status = { textContent: 'Loading form…' };
  const root = { dataset: { kind: 'new' }, querySelector: () => status };
  context.tab = { dataset: { repoType: 'new' }, closest: () => root };
  await vm.runInContext('switchType(tab)', context);
  assert.equal(status.textContent, '');
});
