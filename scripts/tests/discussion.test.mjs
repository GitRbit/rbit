import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';
const source = await readFile(new URL('../../crates/rbit-web/assets/discussion.js', import.meta.url), 'utf8');
const context = vm.createContext({ clearTimeout() {}, document: { addEventListener() {}, querySelector() { return null; }, querySelectorAll() { return []; } } });
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

function discussionPage() {
  const listeners = new Map(), scans = [];
  const scope = (name, contents = {}) => ({
    name,
    matches: () => false,
    querySelectorAll(selector) { scans.push(`${name} ${selector}`); return contents[selector] ?? []; },
  });
  const editor = {};
  const pageButton = { hidden: true };
  const document = Object.assign(scope('document', { '[data-reply]': [pageButton] }), {
    querySelector: selector => selector === '[data-discussion-editor]' ? editor : null,
    addEventListener: (type, listener) => listeners.set(type, listener),
  });
  vm.runInNewContext(source, { clearTimeout() {}, document });
  scans.length = 0;
  const settle = elt => listeners.get('htmx:afterSettle')({ target: elt, detail: { elt } });
  return { scans, scope, settle, pageButton };
}

test('reply buttons in swapped content are enabled without scanning the page', () => {
  const page = discussionPage();
  assert.equal(page.pageButton.hidden, false, 'the loaded page is enhanced');
  const button = { hidden: true };
  page.settle(page.scope('comment', { '[data-reply]': [button] }));
  assert.equal(button.hidden, false);
  assert.ok(page.scans.every(scan => scan.startsWith('comment ')), page.scans.join(', '));
});

test('an editor swapped in enables the reply buttons already on the page', () => {
  const page = discussionPage();
  page.pageButton.hidden = true;
  page.settle(page.scope('form', { '[data-discussion-editor]': [{}] }));
  assert.equal(page.pageButton.hidden, false);
});
