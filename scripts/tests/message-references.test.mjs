import assert from 'node:assert/strict';
import test from 'node:test';
import { validateMessageReferences } from '../message-references.mjs';

const index = new Map([
  ['core.title', { english: 'Languages' }],
  ['core.count', { english: '{count} packs', args: { count: 'number' } }],
]);

test('native snapshot references retain schema validation alongside request-local helpers', () => {
  const source = '{{ crate::i18n::text("core", "title") }} {{ messages.text("core", "title") }} context.locale.text("core", "title")';
  assert.equal(validateMessageReferences(source, 'native.html', index), 3);
  assert.throws(() => validateMessageReferences('messages.text("core", "missing")', 'native.html', index), /unknown/);
  assert.throws(() => validateMessageReferences('context.locale.text("core", "count")', 'native.rs', index), /non-static/);
});

test('translated snapshot output cannot bypass escaping, including formatted messages', () => {
  for (const source of [
    '{{ crate::i18n::text("core", "title")|safe }}',
    '{{ messages.text("core", "title") | safe }}',
    '{{ context.locale.format(&reference) | safe }}',
    '{{ crate::i18n::pack_count(count) | safe }}',
  ]) assert.throws(() => validateMessageReferences(source, 'native.html', index), /must be HTML escaped/);
  assert.equal(validateMessageReferences('{{ messages.format(&reference) }}', 'native.html', index), 0);
});
