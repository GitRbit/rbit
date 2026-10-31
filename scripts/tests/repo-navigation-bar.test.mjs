import assert from 'node:assert/strict';
import test from 'node:test';
import { reconcileRepositoryBar } from '../../crates/rbit-web/assets/repo-navigation-bar.js';

class Node {
  constructor(name, attrs = {}, children = [], document = { activeElement: null }) {
    this.nodeName = name; this.nodeType = name === '#text' ? 3 : 1; this.tagName = this.nodeType === 1 ? name : undefined;
    this.ownerDocument = document; this.parentNode = null; this.childNodes = []; this.attrs = new Map(Object.entries(attrs));
    this.nodeValue = this.nodeType === 3 ? children : null; this.scrollLeft = 0; this.scrollTop = 0;
    this.value = attrs.value ?? ''; this.selectionStart = null; this.selectionEnd = null; this.selectionDirection = 'none';
    this.mutations = 0;
    if (this.nodeType === 1) for (const child of children) this.insertBefore(child, null);
  }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  get id() { return this.getAttribute('id') ?? ''; }
  get classList() { return { contains: name => (this.getAttribute('class') ?? '').split(/\s+/).includes(name) }; }
  get firstChild() { return this.childNodes[0] ?? null; }
  get nextSibling() { return this.parentNode?.childNodes[this.parentNode.childNodes.indexOf(this) + 1] ?? null; }
  get textContent() { return this.nodeType === 3 ? this.nodeValue : this.childNodes.map(node => node.textContent).join(''); }
  get open() { return this.attrs.has('open'); }
  set open(value) { if (value) this.setAttribute('open', ''); else this.removeAttribute('open'); }
  getAttribute(name) { return this.attrs.get(name) ?? null; }
  setAttribute(name, value) { this.mutations++; this.attrs.set(name, value); }
  removeAttribute(name) { this.mutations++; this.attrs.delete(name); }
  insertBefore(node, reference) {
    if (node === reference) return node;
    node.remove();
    const index = reference === null ? this.childNodes.length : this.childNodes.indexOf(reference);
    assert.ok(index >= 0, 'insertBefore reference is a child');
    this.childNodes.splice(index, 0, node); node.parentNode = this; this.mutations++; return node;
  }
  remove() { if (!this.parentNode) return; if (this.contains(this.ownerDocument.activeElement)) this.ownerDocument.activeElement = null; const parent = this.parentNode; parent.childNodes.splice(parent.childNodes.indexOf(this), 1); parent.mutations++; this.parentNode = null; }
  cloneNode(deep) { return new Node(this.nodeName, Object.fromEntries(this.attrs), this.nodeType === 3 ? this.nodeValue : deep ? this.childNodes.map(child => child.cloneNode(true)) : [], this.ownerDocument); }
  contains(node) { return this === node || this.childNodes.some(child => child.contains(node)); }
  querySelectorAll(selector) {
    const matches = node => selector === '*' ? node.nodeType === 1 : selector.startsWith('.') ? node.classList?.contains(selector.slice(1)) : selector.startsWith('[') ? node.getAttribute?.(selector.slice(1, -1)) !== null : node.nodeName === selector.toUpperCase();
    return this.childNodes.flatMap(child => [...(child.nodeType === 1 && matches(child) ? [child] : []), ...child.querySelectorAll(selector)]);
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
  focus(options) { this.ownerDocument.activeElement = this; this.focusOptions = options; }
  setSelectionRange(start, end, direction) { this.selectionStart = start; this.selectionEnd = end; this.selectionDirection = direction; }
}

function fixture({ scope = '/o/r', active = 'code', count = 1, admin = false, clone = 'https://git.example/o/r.git', ssh = false, post = '/o/r/star', label = 'Issues' } = {}) {
  const document = { activeElement: null };
  const text = value => new Node('#text', {}, String(value), document);
  const el = (tag, attrs = {}, children = []) => new Node(tag.toUpperCase(), attrs, children.map(child => typeof child === 'string' ? text(child) : child), document);
  const tabs = ['code', 'issues', 'pulls', 'commits', 'branches', 'tags', 'releases', ...(admin ? ['settings'] : [])].map(key => el('a', { 'data-repo-tab': key, href: `${scope}/${key}`, ...(key === active ? { 'aria-current': 'page' } : {}) }, [key === 'issues' ? label : key, ...(key === 'issues' && count ? [el('span', { class: 'badge' }, [String(count)])] : [])]));
  const nav = el('nav', { class: 'repo-tabs', 'aria-label': 'Repository' }, tabs);
  const cloneInput = el('input', { id: 'clone-url', value: clone, readonly: '', 'aria-label': 'Clone URL' });
  const copy = el('button', { 'data-copy': 'clone-url' }, ['Copy']);
  const menu = el('details', { 'data-menu': '' }, [el('summary', {}, ['Clone']), el('div', { class: 'menu' }, [el('p', {}, ['HTTPS']), el('div', {}, [cloneInput, copy]), ...(ssh ? [el('p', {}, ['SSH']), el('div', {}, [el('input', { id: 'ssh-clone-url', value: 'ssh://git/o/r.git', readonly: '' })])] : []), el('a', { href: `${scope}/archive.zip`, download: '', 'hx-boost': 'false' }, ['Download ZIP'])])]);
  const button = el('button', { 'aria-pressed': String(count > 1) }, ['Star']);
  const form = el('form', { action: post, method: 'post', 'hx-post': post, 'hx-target': '#star-button' }, [el('input', { name: 'next', type: 'hidden', value: `${scope}/${active}` }), button]);
  const star = el('div', { id: 'star-button' }, [form, el('a', { href: `${scope}/stargazers`, 'aria-label': `${count} stargazers` }, [String(count)])]);
  const actions = el('div', { class: 'repo-actions' }, [star, menu]);
  const root = el('div', { id: 'repo-subnav', 'data-repo-scope': scope, class: 'repo-subnav' }, [el('div', { class: 'repo-title' }, [el('a', { href: scope }, ['r'])]), nav, actions]);
  return { root, nav, tabs, menu, cloneInput, copy, star, form, button, document };
}

function withHtmx(run) {
  const previous = globalThis.htmx, processed = [];
  globalThis.htmx = { process: node => processed.push(node) };
  try { run(processed); } finally { globalThis.htmx = previous; }
}

test('same repository navigation preserves bar, tab, action identities and scroll while updating server state', () => withHtmx(processed => {
  const current = fixture(), incoming = fixture({ active: 'issues', count: 7, label: 'Tickets' });
  current.document.activeElement = current.tabs[1]; current.root.scrollLeft = 5; current.root.scrollTop = 2; current.nav.scrollLeft = 180; current.nav.scrollTop = 3;
  assert.equal(reconcileRepositoryBar(current.root, incoming.root), true);
  assert.equal(current.root.querySelector('.repo-tabs'), current.nav);
  assert.deepEqual(current.nav.childNodes, current.tabs);
  assert.equal(current.tabs[0].getAttribute('aria-current'), null);
  assert.equal(current.tabs[1].getAttribute('aria-current'), 'page');
  assert.equal(current.tabs[1].textContent, 'Tickets7');
  assert.equal(current.root.querySelector('.repo-actions'), current.star.parentNode);
  assert.equal(current.star.childNodes[0], current.form);
  assert.equal(current.form.childNodes[1], current.button);
  assert.equal(current.star.childNodes[1].textContent, '7');
  assert.equal(current.button.getAttribute('aria-pressed'), 'true');
  assert.equal(current.form.firstChild.value, '/o/r/issues');
  assert.equal(current.document.activeElement, current.tabs[1]);
  assert.deepEqual([current.root.scrollLeft, current.root.scrollTop, current.nav.scrollLeft, current.nav.scrollTop], [5, 2, 180, 3]);
  assert.equal(processed.includes(current.root), false, 'the stable bar is not broadly reprocessed');
}));

test('permission changes add and remove Settings while retaining every common tab and action', () => withHtmx(processed => {
  const current = fixture();
  assert.equal(reconcileRepositoryBar(current.root, fixture({ admin: true, active: 'settings' }).root), true);
  const settings = current.nav.childNodes.at(-1);
  assert.equal(settings.getAttribute('data-repo-tab'), 'settings');
  assert.equal(settings.getAttribute('aria-current'), 'page');
  assert.ok(processed.includes(settings), 'new settings link is initialized');
  assert.deepEqual(current.nav.childNodes.slice(0, -1), current.tabs);
  assert.equal(reconcileRepositoryBar(current.root, fixture({ active: 'code' }).root), true);
  assert.deepEqual(current.nav.childNodes, current.tabs);
  assert.equal(settings.parentNode, null);
  assert.equal(current.star.parentNode, current.root.querySelector('.repo-actions'));
}));

test('an open clone menu, copy control, focused input and selection survive clone URL and SSH updates', () => {
  const current = fixture(); current.menu.open = true; current.document.activeElement = current.cloneInput;
  current.cloneInput.setSelectionRange(4, 20, 'backward');
  const updated = 'https://new.example/o/r.git';
  reconcileRepositoryBar(current.root, fixture({ clone: updated, ssh: true }).root);
  assert.equal(current.menu.open, true);
  assert.equal(current.root.querySelector('details'), current.menu);
  assert.equal(current.menu.querySelector('button'), current.copy);
  assert.equal(current.menu.querySelector('input'), current.cloneInput);
  assert.equal(current.cloneInput.value, updated);
  assert.equal(current.cloneInput.getAttribute('value'), updated);
  assert.deepEqual([current.cloneInput.selectionStart, current.cloneInput.selectionEnd, current.cloneInput.selectionDirection], [4, 20, 'backward']);
  assert.equal(current.document.activeElement, current.cloneInput);
  assert.equal(current.menu.querySelectorAll('input').length, 2);
  reconcileRepositoryBar(current.root, fixture({ clone: 'short' }).root);
  assert.equal(current.menu.open, true);
  assert.equal(current.menu.querySelectorAll('input').length, 1);
  assert.deepEqual([current.cloneInput.selectionStart, current.cloneInput.selectionEnd], [4, 5]);
});

test('changed hrefs and htmx actions are refreshed, transport attributes and stale bindings removed', () => withHtmx(processed => {
  const current = fixture(), incoming = fixture({ post: '/o/r/unstar' });
  current.tabs[1].setAttribute('hx-get', '/obsolete');
  incoming.tabs[1].setAttribute('href', '/o/r/issues?state=all');
  incoming.root.setAttribute('hx-swap-oob', 'outerHTML');
  reconcileRepositoryBar(current.root, incoming.root);
  assert.equal(current.tabs[1].getAttribute('href'), '/o/r/issues?state=all');
  assert.equal(current.tabs[1].getAttribute('hx-get'), null);
  assert.equal(current.form.getAttribute('hx-post'), '/o/r/unstar');
  assert.equal(current.root.getAttribute('hx-swap-oob'), null);
  assert.ok(processed.includes(current.tabs[1])); assert.ok(processed.includes(current.form));
  assert.ok(!processed.includes(current.tabs[0]));
}));

test('identical SSR chrome performs no attribute, child, or htmx work', () => withHtmx(processed => {
  const current = fixture();
  const all = [current.root, ...current.root.querySelectorAll('*')];
  const before = all.map(node => node.mutations);
  reconcileRepositoryBar(current.root, fixture().root);
  assert.deepEqual(all.map(node => node.mutations), before);
  assert.equal(processed.length, 0);
}));

test('scope mismatch, missing scope, invalid root and duplicate tab keys leave current state untouched', () => {
  for (const mode of ['other', 'missing', 'id', 'tabs', 'duplicate']) {
    const current = fixture(), incoming = fixture({ scope: mode === 'other' ? '/other/repository' : '/o/r', active: 'issues', count: 8 });
    if (mode === 'missing') incoming.root.removeAttribute('data-repo-scope');
    if (mode === 'id') incoming.root.setAttribute('id', 'main');
    if (mode === 'tabs') incoming.nav.remove();
    if (mode === 'duplicate') incoming.tabs[1].setAttribute('data-repo-tab', 'code');
    const children = [...current.root.childNodes], before = current.root.textContent;
    assert.equal(reconcileRepositoryBar(current.root, incoming.root), false, mode);
    assert.deepEqual(current.root.childNodes, children);
    assert.equal(current.root.textContent, before);
    assert.equal(current.tabs[0].getAttribute('aria-current'), 'page');
  }
  assert.equal(reconcileRepositoryBar(null, fixture().root), false);
});


test('reordered keyed tabs retain focus without scrolling or replacing anchors', () => {
  const current = fixture(), incoming = fixture({ active: 'issues' });
  current.document.activeElement = current.tabs[1]; current.nav.scrollLeft = 210;
  incoming.nav.insertBefore(incoming.tabs[1], incoming.tabs[0]);
  assert.equal(reconcileRepositoryBar(current.root, incoming.root), true);
  assert.equal(current.nav.firstChild, current.tabs[1]);
  assert.equal(current.nav.childNodes[1], current.tabs[0]);
  assert.equal(current.document.activeElement, current.tabs[1]);
  assert.deepEqual(current.tabs[1].focusOptions, { preventScroll: true });
  assert.equal(current.nav.scrollLeft, 210);
});

test('repository plugin panels insert, update and disappear without replacing host navigation', () => withHtmx(() => {
  const current = fixture();
  const tab = current.tabs[1], menu = current.menu;
  current.document.activeElement = tab;
  function incoming(text, generation) {
    const next = fixture();
    if (text !== null) {
      const content = new Node('#text', {}, text, next.document);
      const panel = new Node('SECTION', { id: 'plugin-panel-actions', 'data-plugin-owner': 'rbit.actions', 'data-plugin-generation': generation }, [content], next.document);
      next.root.insertBefore(new Node('DIV', { id: 'repo-plugin-panels', class: 'col-span-full' }, [panel], next.document), null);
    }
    return next.root;
  }
  assert.equal(reconcileRepositoryBar(current.root, incoming('First contribution', '1')), true);
  const panel = current.root.querySelectorAll('SECTION')[0];
  assert.ok(panel); assert.equal(panel.textContent, 'First contribution');
  assert.equal(reconcileRepositoryBar(current.root, incoming('Updated contribution', '2')), true);
  assert.equal(current.root.querySelectorAll('SECTION')[0], panel, 'stable panel identity can be reconciled');
  assert.equal(panel.textContent, 'Updated contribution');
  assert.equal(panel.getAttribute('data-plugin-generation'), '2');
  assert.equal(reconcileRepositoryBar(current.root, incoming(null)), true);
  assert.equal(current.root.querySelectorAll('SECTION').length, 0);
  assert.equal(current.root.querySelector('.repo-tabs'), current.nav);
  assert.equal(current.tabs[1], tab); assert.equal(current.menu, menu);
  assert.equal(current.document.activeElement, tab);
}));
