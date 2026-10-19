import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const pickerSource = await readFile(new URL('../../crates/rbit-web/assets/theme-picker.js', import.meta.url), 'utf8');
const fallbackSource = await readFile(new URL('../../crates/rbit-web/assets/theme.js', import.meta.url), 'utf8');

function picker({ slug = 'system', scheme, dark = false } = {}) {
  const listeners = new Map();
  const root = { dataset: slug === 'system' ? {} : { theme: slug, scheme } };
  class Element {}
  class HTMLFormElement extends Element { matches() { return true; } closest() { return null; } }
  class HTMLButtonElement extends Element {
    constructor(value, toggle = true) { super(); this.value = value; this.name = 'theme'; this.dataset = {}; this.toggle = toggle; }
    matches() { return this.toggle; }
  }
  const toggle = new HTMLButtonElement(slug === 'light' ? 'dark' : slug === 'dark' ? 'light' : 'dna-dark');
  const document = {
    documentElement: root, body: {}, cookie: '',
    head: { querySelectorAll: () => [], querySelector: () => null, append() {} },
    querySelectorAll: (selector) => selector === '[data-theme-toggle]' ? [toggle] : [],
    createElement: () => ({}),
    addEventListener: (type, handler) => { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(handler); },
  };
  vm.runInNewContext(pickerSource, { document, Element, HTMLFormElement, HTMLButtonElement, URLSearchParams, location: { protocol: 'https:' }, matchMedia: () => ({ matches: dark }), getComputedStyle: () => ({ backgroundColor: '#141516' }), clearTimeout() {}, fetch: () => Promise.resolve(), localStorage: { getItem: () => null, setItem() {} } });
  const submit = (button = toggle) => listeners.get('submit')[0]({ target: new HTMLFormElement(), submitter: button, preventDefault() {} });
  return { root, toggle, document, submit, choose: (slug) => submit(new HTMLButtonElement(slug, false)), settle: () => listeners.get('htmx:afterSettle')[0]() };
}

test('system header toggle pins opposite DNA variant and keeps the DNA pair through navigation', () => {
  for (const dark of [false, true]) {
    const page = picker({ dark });
    page.submit();
    assert.equal(page.root.dataset.theme, dark ? 'dna-day' : 'dna-dark');
    assert.equal(page.toggle.value, dark ? 'dna-dark' : 'dna-day');
    page.settle();
    page.submit();
    assert.equal(page.root.dataset.theme, dark ? 'dna-dark' : 'dna-day');
    assert.match(page.document.cookie, /rbit-theme=dna-/);
  }
});

test('explicit Classic choices keep their old pair and choosing System restores DNA toggles', () => {
  const page = picker({ slug: 'dark', scheme: 'dark' });
  page.submit();
  assert.equal(page.root.dataset.theme, 'light');
  assert.equal(page.toggle.value, 'dark');
  page.choose('dna-day');
  assert.equal(page.toggle.value, 'dna-dark');
  page.choose('system');
  assert.equal(page.root.dataset.theme, undefined);
  assert.equal(page.toggle.value, 'dna-dark');
});

function fallback(cookie = '', legacy = null) {
  const root = { dataset: {} };
  const links = [];
  const document = { documentElement: root, cookie, createElement: () => ({ dataset: {} }), head: { append: (link) => links.push(link) } };
  vm.runInNewContext(fallbackSource, { document, location: { protocol: 'https:' }, localStorage: { getItem: () => legacy, removeItem() {} } });
  return { root, links, document };
}

test('fallback leaves no-cookie system untouched and restores built-in DNA without fetching a stylesheet', () => {
  assert.deepEqual(fallback().root.dataset, {});
  for (const [slug, scheme] of [['dna-day', 'light'], ['dna-dark', 'dark']]) {
    const page = fallback(`rbit-theme=${slug}`);
    assert.equal(page.root.dataset.theme, slug);
    assert.equal(page.root.dataset.scheme, scheme);
    assert.equal(page.links.length, 0);
  }
});

test('legacy preferences retain Classic palettes and are migrated without relabeling slugs', () => {
  for (const slug of ['light', 'dark']) {
    const page = fallback('', slug);
    assert.equal(page.root.dataset.theme, slug);
    assert.equal(page.root.dataset.scheme, slug);
    assert.equal(page.links.length, 0);
    assert.match(page.document.cookie, new RegExp(`rbit-theme=${slug};`));
    assert.equal(fallback(`rustit-theme=${slug}`).root.dataset.theme, slug);
  }
});
