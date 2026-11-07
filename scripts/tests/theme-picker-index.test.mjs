import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/theme-picker-index.js', import.meta.url), 'utf8');
function mount() {
  const callbacks = new Map();
  const events = new Map();
  let serial = 0;
  let geometryReads = 0;
  const groups = [0, 100, 220].map((offsetTop, index) => ({ id: `group-${index}`, offsetTop, hidden: false }));
  const links = groups.map(group => ({
    dataset: { tpkJump: group.id }, attributes: new Map(), scrolls: 0,
    hasAttribute(name) { return this.attributes.has(name); },
    setAttribute(name, value) { this.attributes.set(name, value); },
    removeAttribute(name) { this.attributes.delete(name); },
    scrollIntoView(options) { assert.equal(options.block, 'nearest'); this.scrolls++; },
  }));
  const list = {
    scrollTop: 0, offsetTop: 0,
    querySelectorAll() { geometryReads++; return groups.filter(group => !group.hidden); },
    querySelector() { return groups.find(group => !group.hidden); },
    addEventListener(type, fn, options) { assert.equal(options.passive, true); events.set(type, fn); },
  };
  const dialog = {
    open: true, isConnected: true,
    querySelector: () => list, querySelectorAll: () => links,
    addEventListener: (type, fn) => events.set(type, fn),
  };
  const context = vm.createContext({
    requestAnimationFrame: fn => { callbacks.set(++serial, fn); return serial; },
    cancelAnimationFrame: id => callbacks.delete(id),
  });
  vm.runInContext(source.replace(/^export /gm, ''), context);
  context.watchIndexScroll(dialog);
  return { dialog, list, links, groups, callbacks, events,
    reads: () => geometryReads,
    sync: () => context.syncIndex(dialog),
    scroll: () => events.get('scroll')(),
    frame() { const current = [...callbacks.values()]; callbacks.clear(); current.forEach(fn => fn()); },
  };
}

test('scroll bursts schedule one pass using the latest geometry and preserve current-link accessibility', () => {
  const page = mount();
  page.sync();
  assert.equal(page.links[0].attributes.get('aria-current'), 'true');
  for (let event = 0; event < 100; event++) { page.list.scrollTop = event * 2; page.scroll(); }
  assert.equal(page.callbacks.size, 1);
  assert.equal(page.reads(), 1);
  page.frame();
  assert.equal(page.reads(), 2);
  assert.equal(page.links[1].attributes.get('aria-current'), 'true');
  assert.equal(page.links[0].attributes.has('aria-current'), false);
  assert.equal(page.links[1].scrolls, 1);
  page.scroll(); page.frame();
  assert.equal(page.links[1].scrolls, 1, 'unchanged selection does not scroll the index again');
});

test('closing cancels work, disconnected callbacks skip geometry, and reopening can schedule again', () => {
  const page = mount();
  page.scroll();
  page.dialog.open = false;
  page.events.get('close')();
  assert.equal(page.callbacks.size, 0);
  page.scroll();
  assert.equal(page.callbacks.size, 0);
  page.dialog.open = true;
  page.scroll();
  page.dialog.isConnected = false;
  page.frame();
  assert.equal(page.reads(), 0);
  page.dialog.isConnected = true;
  page.scroll(); page.frame();
  assert.equal(page.reads(), 1);
});

test('filter/index updates remain synchronous and ignore hidden groups with the original top threshold', () => {
  const page = mount();
  page.list.offsetTop = 20;
  page.groups[1].hidden = true;
  page.list.scrollTop = 198;
  page.sync();
  assert.equal(page.links[0].attributes.get('aria-current'), 'true');
  page.list.scrollTop = 199;
  page.sync();
  assert.equal(page.links[2].attributes.get('aria-current'), 'true');
  page.groups.forEach(group => { group.hidden = true; });
  page.sync();
  assert.ok(page.links.every(link => !link.attributes.has('aria-current')));
});
