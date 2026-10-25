import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';
const source = await readFile(new URL('../../crates/rbit-web/assets/compare.js', import.meta.url), 'utf8');
function harness() {
  const requests = [];
  const listeners = new Map();
  const list = { id: 'refs', children: [], hidden: true, replaceChildren() { this.children = []; }, append(button) { this.children.push(button); } };
  const status = {};
  const form = { elements: { head_repo: { value: 'bob/fork' } }, dataset: {} };
  const container = { querySelector: selector => selector === '[role=listbox]' ? list : status };
  const input = { name: 'head', value: 'fix', isConnected: true, attrs: {}, closest: selector => selector === '[data-compare-field]' ? container : form, setAttribute(k,v) { this.attrs[k] = v; }, removeAttribute(k) { delete this.attrs[k]; } };
  const document = { addEventListener: (name, fn) => listeners.set(name, fn), createElement: () => ({ dataset: {}, setAttribute() {} }) };
  const context = vm.createContext({ document, AbortController, URLSearchParams, encodeURIComponent, clearTimeout, setTimeout, fetch: (url, options) => new Promise(resolve => requests.push({ url, options, resolve })) });
  vm.runInContext(source, context);
  return { requests, input, list, status, form, context };
}
test('compare search follows the head repository and bounds the option list', async () => {
  const h = harness();
  h.context.input = h.input;
  const pending = vm.runInContext('const state = {controller: new AbortController()}; searches.set(input, state); search(input, state)', h.context);
  assert.match(h.requests[0].url, /^\/bob\/fork\/compare\/refs\?q=fix/);
  h.requests[0].resolve({ ok: true, json: async () => Array.from({ length: 60 }, (_, i) => ({ value: `fix${i}`, label: `<fix${i}>`, kind: 'Branch' })) });
  await pending;
  assert.equal(h.list.children.length, 50);
  assert.equal(h.list.children[0].textContent, '<fix0> · Branch');
  assert.equal(h.input.attrs['aria-expanded'], 'true');
});
test('cancelled requests cannot replace newer choices', async () => {
  const h = harness(); h.context.input = h.input;
  const pending = vm.runInContext('const state = {controller: new AbortController()}; searches.set(input, state); search(input, state)', h.context);
  vm.runInContext('cancel(input)', h.context);
  assert.equal(h.requests[0].options.signal.aborted, true);
  h.requests[0].resolve({ ok: true, json: async () => [{ value: 'old' }] });
  await pending;
  assert.equal(h.list.children.length, 0);
});

test('base picker searches its repository and gives branch-only guidance', async () => {
  const h = harness();
  h.input.name = 'base';
  h.form.elements = {};
  h.form.dataset = { repository: 'alice/demo', branchesOnly: 'true' };
  h.context.input = h.input;
  const search = () => vm.runInContext('var state = {controller: new AbortController()}; searches.set(input, state); search(input, state)', h.context);
  let pending = search();
  assert.match(h.requests[0].url, /^\/alice\/demo\/compare\/refs\?q=fix&branches=true$/);
  assert.equal(h.status.textContent, 'Finding branches…');
  h.requests[0].resolve({ ok: true, json: async () => [] });
  await pending;
  assert.equal(h.status.textContent, 'No matching branches. Enter an existing branch name.');
  pending = search();
  h.requests[1].resolve({ ok: false });
  await pending;
  assert.equal(h.status.textContent, 'Search is unavailable. Enter an existing branch name.');
});
