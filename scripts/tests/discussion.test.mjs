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
