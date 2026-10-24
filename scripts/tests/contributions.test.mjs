import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/contributions.js', import.meta.url), 'utf8');

function mount() {
  const listeners = new Map();
  const windowListeners = new Map();
  let documentQueries = 0;
  class Element {
    constructor(dataset = {}) { this.dataset = dataset; this.attributes = {}; this.style = {}; this.offsetWidth = 200; this.offsetHeight = 30; }
    closest(selector) { return selector === '[data-contribution-day]' ? this : selector === 'svg' ? svg : calendar; }
    setAttribute(key, value) { this.attributes[key] = value; }
    querySelector() { return null; }
    getBoundingClientRect() { return { left: 10, top: 80, width: 12, bottom: 92 }; }
    focus() { this.focused = true; }
    replaceChildren(...children) { this.children = children; }
  }
  const days = [
    new Element({ contributionDay: '2024-01-01', count: '2' }),
    new Element({ contributionDay: '2024-01-02', count: '0' }),
    new Element({ contributionDay: '2024-02-01', count: '3' }),
  ];
  const mobileCopy = new Element({ ...days[0].dataset });
  const tooltip = new Element();
  tooltip.hidden = true;
  const details = new Element();
  const svg = { querySelectorAll: () => days };
  const calendar = {
    dataset: { total: '5' },
    querySelector: selector => selector === '[data-contribution-tooltip]' ? tooltip : selector === '[data-contribution-details]' ? details : null,
    querySelectorAll: () => [...days, mobileCopy],
  };
  const document = {
    querySelectorAll: () => { documentQueries++; return [tooltip]; },
    createElement: () => new Element(),
    addEventListener: (name, handler) => listeners.set(name, handler),
  };
  vm.runInNewContext(source, { document, Element, Intl, Date, innerWidth: 320, addEventListener: (name, handler) => windowListeners.set(name, handler) });
  return { days, mobileCopy, tooltip, details, listeners, windowListeners, documentQueries: () => documentQueries };
}

test('calendar hover displays an immediate count tooltip within the viewport', () => {
  const page = mount();
  page.listeners.get('pointerover')({ target: page.days[0] });
  assert.equal(page.tooltip.hidden, false);
  assert.match(page.tooltip.textContent, /2 commits on .*2024/);
  assert.equal(page.tooltip.style.left, '8px');
  page.listeners.get('pointerout')({ target: page.days[0] });
  assert.equal(page.tooltip.hidden, true);
});

test('date selection is exclusive, synchronizes responsive copies, and counts each date once', () => {
  const page = mount();
  page.listeners.get('click')({ target: page.days[0] });
  assert.equal(page.days[0].attributes['aria-pressed'], 'true');
  assert.equal(page.mobileCopy.attributes['aria-pressed'], 'true');
  assert.match(page.details.children[1].textContent, /2 commits in this month · 40.0%/);
  page.listeners.get('click')({ target: page.days[1] });
  assert.equal(page.days[0].attributes['aria-pressed'], 'false');
  assert.equal(page.mobileCopy.attributes['aria-pressed'], 'false');
  assert.equal(page.days[1].attributes['aria-pressed'], 'true');
  assert.match(page.details.children[0].textContent, /0 commits/);
});

test('calendar supports keyboard selection and date navigation', () => {
  const page = mount();
  let prevented = 0;
  page.listeners.get('keydown')({ target: page.days[0], key: 'Enter', preventDefault() { prevented++; } });
  assert.equal(page.days[0].attributes['aria-pressed'], 'true');
  page.listeners.get('keydown')({ target: page.days[0], key: 'ArrowDown', preventDefault() { prevented++; } });
  assert.equal(page.days[1].focused, true);
  assert.equal(prevented, 2);
});

test('scrolling hides a shown tooltip without searching the document', () => {
  const page = mount();
  page.windowListeners.get('scroll')();
  page.listeners.get('pointerover')({ target: page.days[0] });
  assert.equal(page.tooltip.hidden, false);
  page.windowListeners.get('scroll')();
  assert.equal(page.tooltip.hidden, true);
  page.windowListeners.get('scroll')();
  assert.equal(page.documentQueries(), 0);
});
