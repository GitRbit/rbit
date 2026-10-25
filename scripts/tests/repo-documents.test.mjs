import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/repo-documents.js', import.meta.url), 'utf8');
const flush = () => new Promise(resolve => setImmediate(resolve));

function mount({ connection, keyCount = 3, observe = false } = {}) {
  const listeners = new Map(), requests = [], pages = new Map(), timers = new Map();
  let observed, intersection;
  let nextTimer = 0;
  const location = { href: 'https://git.example/o/r/tree/abc', origin: 'https://git.example', hash: '' };
  const state = { htmx: true, scroll: 180 };
  const history = { state, replaceState(next, _, href) { this.state = next; this.href = href; location.href = href; location.hash = new URL(href).hash; } };
  class Element {
    constructor(tag = 'div') { this.tagName = tag.toUpperCase(); this.dataset = {}; this.attrs = {}; this.children = []; this._text = ''; }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this._text = value; this.children = []; }
    cloneNode(deep = false) { const copy = new Element(this.tagName); copy.attrs = { ...this.attrs }; copy.dataset = { ...this.dataset }; copy._text = this._text; if (deep) copy.append(...this.children.map(child => child.cloneNode(true))); return copy; }
    get outerHTML() { return `<${this.tagName}>${this._text}${this.children.map(child => child.outerHTML).join('')}</${this.tagName}>`; }
    getBoundingClientRect() { return { top: this.top ?? 100, bottom: 600, left: 0, right: 1000 }; }
    get id() { return this.attrs.id ?? ''; }
    set id(value) { this.attrs.id = value; }
    setAttribute(key, value) { this.attrs[key] = String(value); if (key.startsWith('data-')) this.dataset[key.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(value); }
    getAttribute(key) { return this.attrs[key] ?? null; }
    removeAttribute(key) { delete this.attrs[key]; }
    append(...children) { for (const child of children) { if (child.parentElement) child.parentElement.children.splice(child.parentElement.children.indexOf(child), 1); child.parentElement = this; this.children.push(child); } }
    replaceChildren(...children) { this._text = ''; this.children = []; this.append(...children); }
    replaceWith(next) { const siblings = this.parentElement.children; siblings[siblings.indexOf(this)] = next; next.parentElement = this.parentElement; this.parentElement = null; }
    get isConnected() { return this === document.body || Boolean(this.parentElement?.isConnected); }
    get previousElementSibling() { const siblings = this.parentElement?.children ?? []; return siblings[siblings.indexOf(this) - 1] ?? null; }
    matches(selector) {
      const tag = selector.match(/^[a-z][a-z0-9]*/i)?.[0];
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
  const keys = ['README.md', 'CONTRIBUTING.md', 'LICENSE', ...Array.from({ length: Math.max(0, keyCount - 3) }, (_, index) => `LICENSE-${index}`)];
  const link = (key, tab = false) => {
    const node = element('a', { 'data-repo-document': key, ...(tab ? { role: 'tab', 'aria-selected': 'false' } : {}) }, key);
    node.href = `https://git.example/o/r/tree/abc?document=${encodeURIComponent(key)}#readme`;
    return node;
  };
  function section(key) {
    const root = element('section', { 'data-repo-documents': '', 'data-document-key': key, 'data-document-scope': '/o/r', id: 'readme' });
    const tabs = element('div', { role: 'tablist' }); tabs.append(...keys.map(name => { const tab = link(name, true); tab.setAttribute('aria-selected', String(name === key)); return tab; }));
    const menu = element('details', { 'data-document-outline': '' }); menu.hidden = true;
    menu.append(element('summary'), element('input', { 'data-document-heading-filter': '' }), element('ul', { 'data-document-headings': '' }), element('p', { 'data-document-no-headings': '' }));
    const panel = element('article', { 'data-document-panel': '', id: 'repo-document-panel' });
    panel.append(element('a', { id: 'intro' }), element('h1', {}, 'Introduction'), element('a', { id: 'quick-start' }), element('h2', {}, 'Quick start'), element('h3', {}, 'Build <script> safely'));
    root.append(tabs, menu, element('div', { 'data-document-status': '' }), panel);
    return root;
  }
  const first = section(keys[0]), sidebar = element('nav'); sidebar.append(...keys.map(key => link(key)));
  document.body.append(first, sidebar);
  vm.runInNewContext(source, { getComputedStyle: () => ({ top: '90px' }), navigator: { connection }, IntersectionObserver: observe ? class { constructor(callback) { intersection = callback; } observe(node) { observed = node; } unobserve() {} disconnect() {} } : undefined, document, Element, URL, location, history, AbortController, addEventListener: listen, CustomEvent: class { constructor(type, options) { this.type = type; this.detail = options?.detail; } }, setTimeout(callback) { const id = ++nextTimer; timers.set(id, callback); return id; }, clearTimeout(id) { timers.delete(id); }, fetch(url, options) { return new Promise((resolve, reject) => requests.push({ url, options, resolve, reject })); }, DOMParser: class { parseFromString(html) { return { querySelector: () => pages.get(html) }; } } });
  const click = (target, extra = {}) => { const event = { target, button: 0, preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; }, ...extra }; fire('click', event); return event; };
  async function respond(index, key, options = {}) { const next = section(key); if (options.size) next.querySelector('[data-document-panel]')._text = 'x'.repeat(options.size); if (options.scope) next.dataset.documentScope = options.scope; next.savedPanel = next.querySelector('[data-document-panel]'); pages.set(String(index), next); requests[index].resolve({ ok: options.ok !== false, url: options.url, headers: { get: () => null }, text: async () => String(index) }); await flush(); return next; }
  return { document, first, sidebar, keys, fire, click, requests, respond, history, state, location, timers, listeners, section, reveal() { intersection([{ target: observed, isIntersecting: true }]); } };
}

test('sidebar replaces only the panel, retains tab header, synchronizes controls, and preserves htmx history state', async () => {
  const page = mount();
  const event = page.click(page.sidebar.children[1]);
  assert.equal(event.prevented, true); assert.equal(event.stopped, true);
  assert.equal(page.listeners.get('click')[0].capture, true);
  assert.equal(page.first.getAttribute('aria-busy'), 'true');
  const next = await page.respond(0, page.keys[1]);
  assert.equal(page.document.querySelector('[data-repo-documents]'), page.first);
  assert.equal(page.first.querySelector('[data-document-panel]'), next.querySelector('[data-document-panel]') ?? next.savedPanel);
  assert.equal(page.sidebar.children[1].getAttribute('aria-current'), 'true');
  assert.equal(page.first.querySelector('[aria-selected="true"]').dataset.repoDocument, page.keys[1]);
  assert.equal(page.history.state, page.state); assert.match(page.history.href, /document=CONTRIBUTING.md#readme$/);
  assert.equal(page.first.scrolled.block, 'start');
  assert.match(page.requests[0].url, /[?&]document_fragment=true$/);
  assert.equal(new URL(page.history.href).searchParams.has('document_fragment'), false);
  assert.equal(page.document.querySelectorAll('[id]').filter(node => node.id === 'intro').length, 1);
});

test('superseded responses and responses after popstate cannot overwrite the current document', async () => {
  const page = mount();
  page.click(page.sidebar.children[1]); page.click(page.sidebar.children[2]);
  assert.equal(page.requests[0].options.signal.aborted, true);
  const latest = await page.respond(1, page.keys[2]); await page.respond(0, page.keys[1]);
  assert.equal(page.first.querySelector('[data-document-panel]'), latest.savedPanel);
  page.click(page.sidebar.children[1]); page.fire('popstate');
  assert.equal(page.requests[2].options.signal.aborted, true);
  await page.respond(2, page.keys[1]);
  assert.equal(page.first.querySelector('[data-document-panel]'), latest.savedPanel);
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
  assert.equal(page.first.querySelector('[aria-selected="true"]').focused, true); assert.equal(prevented, 2);
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
  assert.equal(page.document.querySelector('[data-repo-documents]'), snapshot);
  assert.equal(snapshot.querySelector('[data-document-panel]'), next.savedPanel);
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


test('repeat switches including the original SSR document use cache and preserve header and scroll', async () => {
  const page = mount(), tabs = page.first.querySelectorAll('[role="tab"]');
  const tablist = tabs[0].parentElement;
  page.click(tabs[1]); await page.respond(0, page.keys[1]);
  assert.equal(page.first.scrolled, undefined, 'visible tab activation must not jump the page');
  page.click(tabs[0]); await flush();
  assert.equal(page.requests.length, 1, 'the initial SSR panel was seeded without fetching');
  assert.equal(page.first.dataset.documentKey, page.keys[0]);
  page.click(tabs[1]); await flush();
  assert.equal(page.requests.length, 1, 'a repeat selection must not request HTML again');
  assert.equal(page.first.querySelector('[role="tablist"]'), tablist);
  assert.equal(tabs[1].focused, true, 'focus remains on the original tab node');
  assert.equal(page.first.scrolled, undefined);
  page.first.top = 91;
  page.click(tabs[0]); await flush();
  assert.equal(page.first.scrolled.block, 'start', 'an already aligned sticky header stays at the document start');
  page.click(page.sidebar.children[0]); await flush();
  assert.equal(page.first.scrolled.block, 'start', 'sidebar links still navigate down to documents');
});

test('hover and keyboard focus prefetch share their request with activation', async () => {
  for (const eventName of ['pointerover', 'focusin', 'touchstart']) {
    const page = mount(), tab = page.first.querySelectorAll('[role="tab"]')[1];
    page.fire(eventName, { target: tab });
    assert.equal(page.requests.length, 1);
    assert.equal(page.requests[0].options.priority, 'low');
    page.fire(eventName, { target: tab }); page.click(tab);
    assert.equal(page.requests.length, 1, 'prefetch and click reuse the same in-flight request');
    await page.respond(0, page.keys[1]);
    assert.equal(page.first.dataset.documentKey, page.keys[1]);
    page.click(page.first.querySelectorAll('[role="tab"]')[0]); await flush();
    page.click(tab); await flush();
    assert.equal(page.requests.length, 1);
  }
});

test('speculation is bounded and respects save-data and slow connections', async () => {
  for (const connection of [{ saveData: true }, { effectiveType: '2g' }, { effectiveType: 'slow-2g' }]) {
    const page = mount({ connection, observe: true });
    page.fire('pointerover', { target: page.sidebar.children[1] }); page.reveal();
    assert.equal(page.requests.length, 0);
    page.click(page.sidebar.children[1]);
    assert.equal(page.requests.length, 1, 'explicit activation still loads on constrained connections');
    await page.respond(0, page.keys[1]);
  }
  const page = mount({ keyCount: 12, observe: true });
  for (const target of page.sidebar.children) page.fire('pointerover', { target });
  page.reveal();
  assert.equal(page.requests.length, 2, 'hover and visibility warming together stay within two concurrent speculative loads');
  await page.respond(0, page.keys[1]); await page.respond(1, page.keys[2]);
});

test('nearby document warming fetches at most six alternatives sequentially', async () => {
  const page = mount({ keyCount: 12, observe: true }); page.reveal();
  assert.equal(page.requests.length, 1);
  for (let index = 0; index < 6; index++) {
    assert.equal(page.requests.length, index + 1);
    await page.respond(index, page.keys[index + 1]);
  }
  assert.equal(page.requests.length, 6);
  page.reveal(); await flush();
  assert.equal(page.requests.length, 6, 'visibility does not restart warming');
});

test('cache evicts old documents when count or byte budget is exhausted', async () => {
  for (const oversized of [false, true]) {
    const page = mount({ keyCount: 10 });
    const count = oversized ? 2 : 9;
    for (let index = 0; index < count; index++) {
      page.click(page.sidebar.children[index + 1]);
      await page.respond(index, page.keys[index + 1], { size: oversized ? 2200000 : undefined });
    }
    page.click(page.sidebar.children[0]);
    assert.equal(page.requests.length, count + 1, 'oldest panel was evicted within cache limits');
    await page.respond(count, page.keys[0]);
  }
});

test('failed prefetch retries on activation and navigation invalidates all cached and pending work', async () => {
  const page = mount();
  page.fire('focusin', { target: page.sidebar.children[1] });
  await page.respond(0, page.keys[1], { ok: false });
  page.click(page.sidebar.children[1]);
  assert.equal(page.requests.length, 2, 'a failed speculative load is retryable');
  await page.respond(1, page.keys[1]);
  page.fire('pointerover', { target: page.sidebar.children[2] });
  page.fire('htmx:beforeRequest', { detail: { boosted: true } });
  assert.equal(page.requests[2].options.signal.aborted, true);
  await page.respond(2, page.keys[2]);
  page.click(page.sidebar.children[0]);
  assert.equal(page.requests.length, 4, 'navigation discarded the initial SSR cache');
  await page.respond(3, page.keys[0]);
  page.click(page.sidebar.children[2]);
  assert.equal(page.requests.length, 5, 'late navigation responses cannot repopulate the cache');
  await page.respond(4, page.keys[2]);
});


test('cache keeps pristine Markdown instead of reusing enhanced DOM and stale diagram frames', async () => {
  const page = mount();
  page.click(page.sidebar.children[1]); await page.respond(0, page.keys[1]);
  const mounted = page.first.querySelector('[data-document-panel]');
  const sourceHeading = mounted.querySelector('h1').textContent;
  mounted.querySelector('h1').textContent = 'Enhanced diagram iframe state';
  page.click(page.sidebar.children[0]); await flush();
  page.click(page.sidebar.children[1]); await flush();
  const restored = page.first.querySelector('[data-document-panel]');
  assert.notEqual(restored, mounted);
  assert.equal(restored.querySelector('h1').textContent, sourceHeading);
  assert.equal(page.requests.length, 1);
});


test('navigation stops a visibility warm loop even if its section remains connected', async () => {
  const page = mount({ keyCount: 12, observe: true }); page.reveal();
  assert.equal(page.requests.length, 1);
  page.fire('htmx:beforeRequest', { detail: { boosted: true } });
  assert.equal(page.requests[0].options.signal.aborted, true);
  page.requests[0].reject(new Error('navigation aborted'));
  await flush();
  assert.equal(page.requests.length, 1, 'the old warm loop must not continue into new requests');
});

test('settling swapped content updates its document links without scanning the page', () => {
  const page = mount();
  const nav = page.sidebar.cloneNode(true);
  for (const link of nav.children) link.removeAttribute('aria-current');
  page.document.body.append(nav);
  const { querySelector, querySelectorAll } = page.document;
  page.document.querySelector = page.document.querySelectorAll = () => { throw new Error('settling must not scan the whole page'); };
  page.fire('htmx:afterSettle', { target: nav, detail: { elt: nav } });
  Object.assign(page.document, { querySelector, querySelectorAll });
  assert.deepEqual(nav.children.map(link => link.getAttribute('aria-current')), ['true', null, null]);
  const next = page.section(page.keys[1]);
  page.first.replaceWith(next);
  page.fire('htmx:afterSettle', { target: next, detail: { elt: next } });
  assert.deepEqual(nav.children.map(link => link.getAttribute('aria-current')), [null, 'true', null], 'a new section updates links elsewhere on the page');
});
