import assert from 'node:assert/strict';
import test from 'node:test';
import { installUpdateNotice } from '../../crates/rbit-web/assets/update-notice.js';

const release = 'https://github.com/GitRbit/rbit/releases/tag/v99.0.0';
function fixture(initial = '1000000000000000001;pending') {
  const listeners = new Map();
  const placeholder = raw => ({
    raw, children: [],
    getAttribute() { return this.raw; }, setAttribute(name, value) { this.raw = value; },
    querySelector() { return this.children[0] ?? null; },
    replaceChildren(...children) { this.children = children; },
  });
  const doc = {
    target: placeholder(initial),
    querySelector() { return this.target; },
    createElement() { return { setAttribute(name, value) { this[name] = value; } }; },
    addEventListener(name, callback) { const callbacks = listeners.get(name) ?? []; callbacks.push(callback); listeners.set(name, callbacks); },
  };
  const dispatch = (name, detail = {}) => { for (const callback of listeners.get(name) ?? []) callback({ detail }); };
  installUpdateNotice(doc);
  const response = raw => dispatch('htmx:afterRequest', { successful: true, xhr: { getResponseHeader: () => raw } });
  return { doc, response, dispatch, restore(raw) { doc.target = placeholder(raw); dispatch('htmx:historyRestore'); } };
}

test('pending footer learns later available snapshot and preserves it through cached tabs and history', () => {
  const a = fixture();
  assert.equal(a.doc.target.children.length, 0);
  a.response(`1000000000000000002;${release}`);
  assert.equal(a.doc.target.children[0].href, release);
  assert.equal(a.doc.target.children[0].textContent, 'Update available');
  a.response('1000000000000000001;none');
  a.restore('1000000000000000001;pending');
  assert.equal(a.doc.target.children[0].href, release);
  a.response('1000000000000000003;pending');
  assert.equal(a.doc.target.children[0].href, release);
});

test('newer cached snapshots supersede live clears while older cached notices cannot resurrect them', () => {
  const a = fixture(`1000000000000000001;${release}`);
  a.response('1000000000000000002;none');
  assert.equal(a.doc.target.children.length, 0);
  a.response(`1000000000000000001;${release}`);
  assert.equal(a.doc.target.children.length, 0);
  a.response(`1000000000000000003;${release}`);
  assert.equal(a.doc.target.children[0].href, release);
  a.restore('1000000000000000004;none');
  assert.equal(a.doc.target.children.length, 0);
  a.response(`1000000000000000003;${release}`);
  assert.equal(a.doc.target.children.length, 0);
});

test('unsafe or malformed notice metadata cannot create links or suppress later valid snapshots', () => {
  const a = fixture();
  for (const raw of ['bad', '9;javascript:alert(1)', '9999999999999999999;https://evil.example', `9999999999999999999;${release}\n`]) a.response(raw);
  assert.equal(a.doc.target.children.length, 0);
  a.response(`1000000000000000002;${release}`);
  assert.equal(a.doc.target.children[0].href, release);
});
