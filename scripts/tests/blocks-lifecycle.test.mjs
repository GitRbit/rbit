import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/blocks.js', import.meta.url), 'utf8');
function mount(typedAttr = false) {
  const listeners = new Map();
  const measured = [];
  const written = [];
  const targets = new Set();
  let observer;
  class Element {
    constructor(rows, width = 300) {
      this.dataset = { rows };
      this.clientWidth = width;
      this.children = [];
      this.style = {
        set containIntrinsicBlockSize(value) { written.push(value); },
        removeProperty(name) { written.push(`remove:${name}`); },
      };
    }
    matches(selector) { return this.dataset.rows !== undefined && selector.includes('.diff-file'); }
    querySelectorAll() { measured.push(this); return this.children; }
    querySelector() { return { minWidth: 'max(100%, 600px)' }; }
  }
  const document = {
    body: { append() {} },
    createElement: () => ({ style: {}, offsetHeight: 80, clientHeight: 60, remove() {} }),
    querySelectorAll: () => [],
    addEventListener: (type, fn) => listeners.set(type, fn),
  };
  vm.runInNewContext(source, {
    document, Element, CSS: { supports: () => typedAttr },
    getComputedStyle: view => view,
    ResizeObserver: class {
      constructor(callback) { observer = callback; }
      observe(target) { targets.add(target); }
      unobserve(target) { targets.delete(target); }
    },
  });
  const dispatch = (type, elt) => listeners.get(type)({ detail: { elt } });
  function column() {
    const column = new Element(undefined);
    const file = new Element('5');
    column.children.push(file);
    file.parentElement = column;
    return { column, file };
  }
  return { column, targets, written, measured, dispatch,
    resize: (target, width) => observer([{ target, contentRect: { width } }]),
  };
}

test('cleanup releases each removed column and ignores queued resize deliveries', () => {
  const page = mount();
  for (let navigation = 0; navigation < 100; navigation++) {
    const { column, file } = page.column();
    page.dispatch('htmx:load', column);
    assert.equal(page.targets.size, 1);
    page.dispatch('htmx:beforeCleanupElement', column);
    page.dispatch('htmx:beforeCleanupElement', file);
    assert.equal(page.targets.size, 0);
    const reads = page.measured.length;
    const writes = page.written.length;
    page.resize(column, 100);
    page.resize(column, 200);
    assert.equal(page.measured.length, reads);
    assert.equal(page.written.length, writes);
  }
});

test('restored columns are reobserved with fresh width tracking and unchanged scrollbar sizing', () => {
  const page = mount();
  const { column, file } = page.column();
  page.dispatch('htmx:load', column);
  assert.equal(page.written.at(-1), 'auto calc(5 * var(--row-h) + 20px)');
  page.resize(column, 300);
  const writes = page.written.length;
  page.resize(column, 300);
  assert.equal(page.written.length, writes);
  file.clientWidth = 700;
  page.resize(column, 700);
  assert.equal(page.written.at(-1), 'auto calc(5 * var(--row-h) + 0px)');
  page.dispatch('htmx:beforeCleanupElement', column);
  page.dispatch('htmx:load', column);
  assert.ok(page.targets.has(column));
  const restoredWrites = page.written.length;
  page.resize(column, 700);
  assert.equal(page.written.length, restoredWrites);
  file.clientWidth = 300;
  page.resize(column, 300);
  assert.equal(page.written.at(-1), 'auto calc(5 * var(--row-h) + 20px)');
});

test('typed attr sizing still removes the override when no scrollbar space is needed', () => {
  const page = mount(true);
  const { column, file } = page.column();
  file.clientWidth = 700;
  page.dispatch('htmx:load', column);
  assert.equal(page.written.at(-1), 'remove:contain-intrinsic-block-size');
});
