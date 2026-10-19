import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/repo-documents.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));

function mount() {
  const listeners = new Map(), requests = [], pages = new Map(), timers = new Map();
  const location = { href: 'https://git.example/o/r/tree/abc', origin: 'https://git.example', hash: '' };
  const state = { htmx: true, scroll: 180 };
  const history = { state, replaceState(next, _, href) { this.state = next; this.href = href; location.href = href; location.hash = new URL(href).hash; } };
  class Element {
    constructor(tag = 'div') { this.tagName = tag.toUpperCase(); this.dataset = {}; this.attrs = {}; this.children = []; this._text = ''; }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this._text = value; this.children = []; }
    get id() { return this.attrs.id ?? ''; }
    set id(value) { this.attrs.id = value; }
    setAttribute(key, value) { this.attrs[key] = String(value); if (key.startsWith('data-')) this.dataset[key.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(value); }
    getAttribute(key) { return this.attrs[key] ?? null; }
    removeAttribute(key) { delete this.attrs[key]; }
    append(...children) { for (const child of children) { child.parentElement = this; this.children.push(child); } }
    replaceChildren(...children) { this._text = ''; this.children = []; this.append(...children); }
    replaceWith(next) { const siblings = this.parentElement.children; siblings[siblings.indexOf(this)] = next; next.parentElement = this.parentElement; this.parentElement = null; }
    get isConnected() { return this === document.body || Boolean(this.parentElement?.isConnected); }
    get previousElementSibling() { const siblings = this.parentElement?.children ?? []; return siblings[siblings.indexOf(this) - 1] ?? null; }
    matches(selector) {
      const tag = selector.match(/^[a-z]+/i)?.[0];
      if (tag && this.tagName !== tag.toUpperCase()) return false;
      for (const match of selector.matchAll(/\[([^=\]]+)(?:="([^"]*)")?\]/g)) {
        const value = match[1].startsWith('data-') ? this.dataset[match[1].slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] : this.getAttribute(match[1]);
        if (value == null || (match[2] !== undefined && value !== match[2])) return false;
      }
      return true;
    }
    closest(selector) { return this.matches(selector) ? this : this.parentElement?.closest(selector) ?? null; }
    querySelectorAll(selector) {
      const descendants = this.children.flatMap(child => [child, ...child.querySelectorAll('*')]);
      if (selector.includes(':is(')) return descendants.filter(child => /^H[1-6]$/.test(child.tagName) && child.closest('[data-document-panel]'));
      return descendants.filter(child => child.matches(selector));
    }
    querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
    focus() { this.focused = true; }
    scrollIntoView(options) { this.scrolled = options; }
  }
  const fire = (type, event = {}) => { for (const listener of listeners.get(type) ?? []) listener.handler(event); };
  const listen = (type, handler, capture) => { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push({ handler, capture }); };
  const document = {
    body: new Element('body'),
    querySelector(selector) { return this.body.querySelector(selector); },
    querySelectorAll(selector) { return this.body.querySelectorAll(selector); },
    getElementById(id) { return this.body.querySelectorAll('[id]').find(element => element.id === id); },
    createElement: tag => new Element(tag),
    createTextNode: text => { const node = new Element('text'); node.textContent = text; return node; },
    addEventListener: listen,
    dispatchEvent(event) { fire(event.type, event); },
  };
  const element = (tag, attrs = {}, text = '') => { const node = new Element(tag); for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value); node.textContent = text; return node; };
  const keys = ['README.md', 'CONTRIBUTING.md', 'LICENSE'];
  const link = (key, tab = false) => {
    const node = element('a', { 'data-repo-document': key, ...(tab ? { role: 'tab', 'aria-selected': 'false' } : {}) }, key);
    node.href = `https://git.example/o/r/tree/abc?document=${encodeURIComponent(key)}#readme`;
    return node;
  };
  function section(key) {
    const root = element('section', { 'data-repo-documents': '', 'data-document-key': key, 'data-document-scope': '/o/r', id: 'readme' });
    const tabs = element('div', { role: 'tablist' }); tabs.append(...keys.map(key => link(key, true)));
    const menu = element('details', { 'data-document-outline': '' }); menu.hidden = true;
    menu.append(element('summary'), element('input', { 'data-document-heading-filter': '' }), element('ul', { 'data-document-headings': '' }), element('p', { 'data-document-no-headings': '' }));
    const panel = element('article', { 'data-document-panel': '', id: 'repo-document-panel' });
    panel.append(element('a', { id: 'intro' }), element('h1', {}, 'Introduction'), element('a', { id: 'quick-start' }), element('h2', {}, 'Quick start'), element('h3', {}, 'Build <script> safely'));
    root.append(tabs, menu, element('div', { 'data-document-status': '' }), panel);
    return root;
  }
  const first = section(keys[0]), sidebar = element('nav'); sidebar.append(...keys.map(key => link(key)));
  document.body.append(first, sidebar);
  vm.runInNewContext(source, { document, Element, URL, location, history, AbortController, addEventListener: listen, CustomEvent: class { constructor(type, options) { this.type = type; this.detail = options?.detail; } }, setTimeout(callback) { const id = timers.size + 1; timers.set(id, callback); return id; }, clearTimeout(id) { timers.delete(id); }, fetch(url, options) { return new Promise((resolve, reject) => requests.push({ url, options, resolve, reject })); }, DOMParser: class { parseFromString(html) { return { querySelector: () => pages.get(html) }; } } });
  const click = (target, extra = {}) => { const event = { target, button: 0, preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; }, ...extra }; fire('click', event); return event; };
  async function respond(index, key, options = {}) { const next = section(key); if (options.scope) next.dataset.documentScope = options.scope; pages.set(String(index), next); requests[index].resolve({ ok: options.ok !== false, url: options.url, headers: { get: () => null }, text: async () => String(index) }); await flush(); return next; }
  return { document, first, sidebar, keys, fire, click, requests, respond, history, state, location, timers, listeners, section };
}

test('sidebar switches only document section, synchronizes controls, and preserves htmx history state', async () => {
  const page = mount();
  const event = page.click(page.sidebar.children[1]);
  assert.equal(event.prevented, true); assert.equal(event.stopped, true);
  assert.equal(page.listeners.get('click')[0].capture, true);
  assert.equal(page.first.getAttribute('aria-busy'), 'true');
  const next = await page.respond(0, page.keys[1]);
  assert.equal(page.document.querySelector('[data-repo-documents]'), next);
  assert.equal(page.sidebar.children[1].getAttribute('aria-current'), 'true');
  assert.equal(next.querySelector('[aria-selected="true"]').dataset.repoDocument, page.keys[1]);
  assert.equal(page.history.state, page.state); assert.match(page.history.href, /document=CONTRIBUTING.md#readme$/);
  assert.equal(next.scrolled.block, 'start');
  assert.equal(page.document.querySelectorAll('[id]').filter(node => node.id === 'intro').length, 1);
});

test('superseded responses and responses after popstate cannot overwrite the current document', async () => {
  const page = mount();
  page.click(page.sidebar.children[1]); page.click(page.sidebar.children[2]);
  assert.equal(page.requests[0].options.signal.aborted, true);
  const latest = await page.respond(1, page.keys[2]); await page.respond(0, page.keys[1]);
  assert.equal(page.document.querySelector('[data-repo-documents]'), latest);
  page.click(page.sidebar.children[1]); page.fire('popstate');
  assert.equal(page.requests[2].options.signal.aborted, true);
  await page.respond(2, page.keys[1]);
  assert.equal(page.document.querySelector('[data-repo-documents]'), latest);
});

test('failed, cross-repository, redirected, and timed-out responses retain content with full-page fallback', async () => {
  for (const failure of ['http', 'scope', 'redirect', 'timeout']) {
    const page = mount(); page.click(page.sidebar.children[1]);
    if (failure === 'timeout') { [...page.timers.values()][0](); page.requests[0].reject(new Error('Aborted')); await flush(); }
    else await page.respond(0, page.keys[1], { ok: failure !== 'http', scope: failure === 'scope' ? '/other/repo' : undefined, url: failure === 'redirect' ? 'https://git.example/other/repo' : undefined });
    assert.equal(page.document.querySelector('[data-repo-documents]'), page.first);
    const status = page.first.querySelector('[data-document-status]');
    assert.match(status.textContent, /Unable to load/); assert.equal(status.querySelector('a').href, page.sidebar.children[1].href);
    assert.equal(page.first.getAttribute('aria-busy'), null);
  }
});

test('keyboard roves tab focus and Space activates; modified clicks retain normal navigation', async () => {
  const page = mount(), tabs = page.first.querySelectorAll('[role="tab"]');
  let prevented = 0;
  page.fire('keydown', { target: tabs[0], key: 'ArrowRight', preventDefault() { prevented++; } });
  assert.equal(tabs[1].focused, true); assert.equal(tabs[1].tabIndex, 0); assert.equal(tabs[0].tabIndex, -1); assert.equal(page.requests.length, 0);
  page.fire('keydown', { target: tabs[1], key: ' ', preventDefault() { prevented++; } });
  const next = await page.respond(0, page.keys[1]);
  assert.equal(next.querySelector('[aria-selected="true"]').focused, true); assert.equal(prevented, 2);
  assert.equal(page.click(page.sidebar.children[2], { ctrlKey: true }).prevented, undefined); assert.equal(page.requests.length, 1);
});

test('outline reuses preceding anchors, safely labels headings, indents and filters, preserving pinned URL', () => {
  const page = mount(), menu = page.first.querySelector('[data-document-outline]');
  assert.equal(menu.hidden, false);
  const items = page.first.querySelector('[data-document-headings]').children;
  assert.deepEqual(items.map(item => item.dataset.headingDepth), ['0', '1', '2']);
  assert.match(items[0].children[0].href, /#intro$/); assert.equal(items[2].children[0].textContent, 'Build <script> safely');
  const input = page.first.querySelector('[data-document-heading-filter]'); input.value = ' QUICK   start ';
  page.fire('input', { target: input }); assert.deepEqual(items.map(item => item.hidden), [true, false, true]);
  page.click(page.sidebar.children[0]); menu.open = true;
  assert.match(items[1].children[0].href, /document=README.md#quick-start$/, 'copy link and modified clicks also use the pinned document URL');
  page.click(items[1].children[0]);
  assert.equal(menu.open, false); assert.equal(page.document.getElementById('quick-start').focused, true);
  assert.match(page.history.href, /document=README.md#quick-start$/); assert.equal(page.history.state, page.state);
});

test('history restore rebuilds outline references and reconciles stale snapshots with current URL', async () => {
  const page = mount(), snapshot = page.section(page.keys[0]);
  page.first.replaceWith(snapshot); page.location.href = page.sidebar.children[2].href; page.location.hash = '#readme';
  page.fire('htmx:historyRestore');
  assert.equal(snapshot.querySelector('[data-document-headings]').children.length, 3);
  const next = await page.respond(0, page.keys[2]);
  assert.equal(page.document.querySelector('[data-repo-documents]'), next);
  assert.equal(page.history.href, undefined);
});

test('a very large heading list stays bounded and initialization survives content-loaded events', () => {
  const page = mount(), next = page.section(page.keys[0]);
  const heading = next.querySelectorAll('[data-document-panel] :is(h1,h2,h3,h4,h5,h6)')[0];
  heading.id = 'existing-heading';
  const original = next.querySelectorAll.bind(next);
  next.querySelectorAll = selector => selector.includes(':is(') ? Array(150000).fill(heading) : original(selector);
  page.first.replaceWith(next);
  page.fire('rbit:content-loaded');
  assert.equal(next.querySelector('[data-document-headings]').children.length, 500);
  page.fire('htmx:afterSettle');
  assert.equal(next.querySelector('[data-document-headings]').children.length, 500, 'repeat initialization does not duplicate menu entries');
});
