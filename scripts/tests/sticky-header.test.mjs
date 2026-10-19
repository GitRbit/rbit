import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/sticky-header.js', import.meta.url), 'utf8');

function page() {
  const values = new Map();
  const handlers = new Map();
  const watched = new Set();
  const bar = height => ({ isConnected: true, height, getBoundingClientRect() { return { height: this.height }; } });
  const current = [bar(48.8), bar(43.2)];
  let resize;
  vm.runInNewContext(source, {
    document: {
      documentElement: { style: { setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key) } },
      querySelector: selector => current[selector.endsWith('.site-header') ? 0 : 1],
      addEventListener: (type, handler) => handlers.set(type, handler),
    },
    ResizeObserver: class {
      constructor(callback) { resize = callback; }
      observe(element) { watched.add(element); }
      unobserve(element) { watched.delete(element); }
    },
  });
  return { values, handlers, watched, current, bar, resize };
}

test('sticky offsets retain fractional bar heights and follow real size changes', () => {
  const state = page();
  assert.equal(state.values.get('--bar-height'), '48.8px');
  assert.equal(state.values.get('--subnav-height'), '43.2px');
  state.current[0].height = 61.6;
  state.current[1].height = 0;
  state.resize();
  assert.equal(state.values.get('--bar-height'), '61.6px');
  assert.equal(state.values.get('--subnav-height'), '0px');
});

test('navigation and history restoration reconnect observers and clear missing bars', () => {
  const state = page();
  const oldHeader = state.current[0];
  state.current[0] = state.bar(52.4);
  state.handlers.get('htmx:historyRestore')();
  assert.equal(state.watched.has(oldHeader), false);
  assert.equal(state.watched.has(state.current[0]), true);
  assert.equal(state.values.get('--bar-height'), '52.4px');
  state.current[1] = null;
  state.handlers.get('htmx:afterSettle')();
  assert.equal(state.watched.size, 1);
  assert.equal(state.values.has('--subnav-height'), false);
});
