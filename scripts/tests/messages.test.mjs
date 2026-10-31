import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = (await readFile(new URL('../../crates/rbit-web/assets/messages.js', import.meta.url), 'utf8')).replace(/^export /gm, '');
function app(catalog, lang = 'fr') {
  const document = { body: { dataset: { browserMessages: typeof catalog === 'string' ? catalog : JSON.stringify(catalog) } }, documentElement: { lang } };
  const context = { document, Intl };
  vm.runInNewContext(source, context);
  return { document, messages: context.rbitMessages };
}

test('localized browser text uses the request catalog and bounded named arguments as text', () => {
  const { messages } = app({ 'browser-greeting': 'Bonjour \uE000name\uE001', 'browser-count': '\uE000count\uE001 éléments' });
  assert.equal(messages.text('browser-greeting', { name: '<img src=x onerror=alert(1)>' }), 'Bonjour <img src=x onerror=alert(1)>');
  assert.equal(messages.text('browser-count', { count: 3 }), '3 éléments');
  assert.equal(messages.text('browser-greeting', { name: { toString() { throw new Error('must not execute'); } } }), 'Bonjour ');
  assert.equal(messages.text('browser-missing'), undefined, 'caller retains its English fallback');
  assert.equal(messages.text('browser-greeting', { name: 'x'.repeat(9000) }).length, 2056);
  assert.equal(Object.isFrozen(messages), true);
});

test('catalog replacement after a DOM navigation never reuses another locale snapshot', () => {
  const { document, messages } = app({ 'browser-close': 'Fermer' });
  assert.equal(messages.text('browser-close'), 'Fermer');
  document.body.dataset.browserMessages = JSON.stringify({ 'browser-close': 'إغلاق' });
  document.documentElement.lang = 'ar';
  assert.equal(messages.text('browser-close'), 'إغلاق');
  assert.equal(messages.locale(), 'ar');
  assert.equal(messages.number(1234), new Intl.NumberFormat('ar').format(1234));
});

test('malformed and oversized catalogs leave enhancements on their ordinary fallback', () => {
  for (const catalog of ['{', '[]', 'null', 'x'.repeat(65537), JSON.stringify({ 'browser-close': 'x'.repeat(8193) })]) {
    assert.equal(app(catalog).messages.text('browser-close'), undefined);
  }
  const { messages } = app({ 'browser-close': 'Fermer' }, 'not_a_locale');
  assert.equal(messages.number(12), '12', 'invalid locale cannot break controls');
});


test('native locale plural rules select zero, one, two, few, many and other browser messages', () => {
  const catalog = Object.fromEntries(['zero', 'one', 'two', 'few', 'many', 'other'].map(category => [`browser-count-${category}`, `${category}: \uE000count\uE001`]));
  const { messages } = app(catalog, 'ar');
  for (const [count, category] of [[0,'zero'],[1,'one'],[2,'two'],[3,'few'],[11,'many'],[100,'other']]) {
    assert.equal(messages.plural('browser-count', count), `${category}: ${new Intl.NumberFormat('ar').format(count)}`);
  }
});
