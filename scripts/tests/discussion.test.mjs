import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';
const source = await readFile(new URL('../../crates/rbit-web/assets/discussion.js', import.meta.url), 'utf8');
const context = vm.createContext({ clearTimeout() {}, document: { addEventListener() {}, querySelector() { return null; } } });
vm.runInContext(source, context);
const mention = (text, caret = text.length) => vm.runInContext(`mentionAt(${JSON.stringify(text)}, ${caret})`, context);
test('mentions follow the caret without matching mail addresses or cross-repo references', () => {
  assert.equal(mention('hello @ali').query, 'ali');
  assert.equal(mention('#12').accounts, false);
  assert.equal(mention('a@example.com'), null);
  assert.equal(mention('owner/repo#12'), null);
  assert.equal(mention('hello @alice more', 9).query, 'al');
  assert.equal(mention('hello @alice '), null);
});
test('replies quote every source line and keep a stable comment link', () => {
  const quote = vm.runInContext(`quoteReply('alice', 'issuecomment-4', 'one\\n\\ntwo')`, context);
  assert.equal(quote, '[Replying to @alice](#issuecomment-4)\n\n> one\n> \n> two\n\n');
  assert.ok(vm.runInContext(`quoteReply('', 'issue-body', 'x'.repeat(5000))`, context).length < 4100);
});

test('choosing a mention preserves text after the caret and respects the comment limit', () => {
  const list = { hidden: false }, status = {};
  const input = { value: 'Hi @ali please check', selectionStart: 7, maxLength: 65536,
    form: { querySelector: selector => selector === '[role=listbox]' ? list : status },
    focus() {}, removeAttribute() {},
    setRangeText(value, start, end) { this.value = this.value.slice(0, start) + value + this.value.slice(end); },
  };
  context.input = input;
  vm.runInContext("insert(input, {dataset: {mention: '@alice'}})", context);
  assert.equal(input.value, 'Hi @alice  please check');
  input.value = 'Hi @ali'; input.maxLength = input.value.length;
  vm.runInContext("insert(input, {dataset: {mention: '@alice'}})", context);
  assert.equal(input.value, 'Hi @ali');
  assert.match(status.textContent, /comment limit/);
});

test('Preview focus closes mention choices and a late lookup cannot reopen them', async () => {
  const handlers = {}, option = {}, list = { hidden: false, contains: target => target === option, replaceChildren() {}, append() {} };
  let removed = false, resolveResponse, signal;
  const input = { value: '#1', selectionStart: 2, isConnected: true, dataset: { suggestions: '/suggestions' },
    form: { querySelector: () => list }, removeAttribute(name) { removed = name === 'aria-activedescendant'; },
  };
  const document = { querySelector: () => input, querySelectorAll: () => [], addEventListener(name, callback) { handlers[name] = callback; } };
  const context = vm.createContext({ input, document, clearTimeout() {}, AbortController, URLSearchParams,
    fetch(_url, options) { signal = options.signal; return new Promise(resolve => { resolveResponse = resolve; }); },
  });
  vm.runInContext(source, context);
  const lookup = vm.runInContext('suggestions(input, revision)', context);
  handlers.focusin({ target: input });
  handlers.focusin({ target: option });
  assert.equal(list.hidden, false, 'editor and option focus retain the choices for selection');
  assert.equal(signal.aborted, false, 'moving into an option does not cancel its lookup');
  handlers.focusin({ target: { textContent: 'Preview' } });
  assert.equal(list.hidden, true);
  assert.equal(removed, true, 'closing removes the active option reference');
  assert.equal(signal.aborted, true, 'leaving the editor aborts pending suggestions');
  resolveResponse({ ok: true, json: async () => [['#1', 'A saved issue']] });
  await lookup;
  assert.equal(list.hidden, true, 'even a response that ignores abort cannot reopen the list');
});