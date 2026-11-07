import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const pickerModule = (await readFile(new URL('../../crates/rbit-web/assets/theme-picker.js', import.meta.url), 'utf8')).replaceAll('\r\n', '\n');
const indexModule = await readFile(new URL('../../crates/rbit-web/assets/theme-picker-index.js', import.meta.url), 'utf8');
const indexImport = 'import { syncIndex, watchIndexScroll } from "./theme-picker-index.js";';
const labelImport = 'import { pickerCount, pickerCredit, pickerApply } from "./theme-picker-labels.js";';
const labelModule = await readFile(new URL('../../crates/rbit-web/assets/theme-picker-labels.js', import.meta.url), 'utf8');
assert.ok(pickerModule.startsWith(labelImport + "\n" + indexImport));
const pickerSource = indexModule.replace(/^export /gm, '') + labelModule.replace(/^export /gm, '')
  + pickerModule.slice(labelImport.length + 1 + indexImport.length);
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
  const manifest = {};
  const document = {
    documentElement: root, body: {}, cookie: '',
    head: { querySelectorAll: () => [], querySelector: selector => selector === 'link[rel="manifest"]' ? manifest : null, append() {} },
    querySelectorAll: (selector) => selector === '[data-theme-toggle]' ? [toggle] : [],
    createElement: () => ({}),
    addEventListener: (type, handler) => { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(handler); },
  };
  const matchesFilter = vm.runInNewContext(pickerSource + "\nmatchesPickerFilter;", { document, Element, HTMLFormElement, HTMLButtonElement, URLSearchParams, location: { protocol: 'https:' }, matchMedia: () => ({ matches: dark }), getComputedStyle: () => ({ backgroundColor: '#141516' }), clearTimeout() {}, fetch: () => Promise.resolve(), localStorage: { getItem: () => null, setItem() {} } });
  const submit = (button = toggle) => listeners.get('submit')[0]({ target: new HTMLFormElement(), submitter: button, preventDefault() {} });
  return { root, toggle, document, manifest, matchesFilter, submit, choose: (slug) => submit(new HTMLButtonElement(slug, false)), settle: () => listeners.get('htmx:afterSettle')[0]() };
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

test('app manifest follows applied themes and the device scheme for System', () => {
  const page = picker({ dark: true });
  page.choose('dna-day');
  assert.equal(page.manifest.href, '/manifest.webmanifest?theme=dna-day');
  page.choose('system');
  assert.equal(page.manifest.href, '/manifest.webmanifest?theme=dna-dark');
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

for (const [slug, scheme, sibling] of [
  ['colorblind-light', 'light', 'colorblind-dark'],
  ['colorblind-dark', 'dark', 'colorblind-light'],
]) {
  test(`${slug} restores offline and retains its sibling through toggles and navigation`, () => {
    const restored = fallback(`rbit-theme=${slug}`);
    assert.equal(restored.root.dataset.theme, slug);
    assert.equal(restored.root.dataset.scheme, scheme);
    assert.equal(restored.links.length, 0);
    const page = picker();
    page.choose(slug);
    assert.equal(page.root.dataset.scheme, scheme);
    assert.equal(page.toggle.value, sibling);
    assert.equal(page.manifest.href, `/manifest.webmanifest?theme=${slug}`);
    page.submit();
    assert.equal(page.root.dataset.theme, sibling);
    page.settle();
    page.submit();
    assert.equal(page.root.dataset.theme, slug);
    assert.match(page.document.cookie, new RegExp(`rbit-theme=${slug};`));
  });
}

const css = await readFile(new URL('../../crates/rbit-web/styles/app.css', import.meta.url), 'utf8');
const tokens = (slug) => Object.fromEntries([...css.match(new RegExp(`\\[data-theme="${slug}"\\] \\{([^}]+)\\}`))[1].matchAll(/--([a-z-]+): (#[0-9a-f]{6});/g)].map(([, key, value]) => [key, value]));
function luminance(hex) {
  const channels = hex.slice(1).match(/../g).map((c) => parseInt(c, 16) / 255).map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  return channels.reduce((sum, c, i) => sum + c * [.2126, .7152, .0722][i], 0);
}
function contrast(a, b) {
  const [x, y] = [luminance(a), luminance(b)].sort((a, b) => b - a);
  return (x + .05) / (y + .05);
}
for (const [slug, parent] of [['colorblind-light', 'dna-day'], ['colorblind-dark', 'dna-dark']]) {
  test(`${slug} preserves rbit surfaces and readable text, syntax and status colours`, () => {
    const t = tokens(slug), original = tokens(parent);
    for (const name of ['canvas', 'surface', 'subtle', 'hover', 'line']) assert.equal(t[name], original[name]);
    for (const name of ['ink', 'ink-muted', 'ink-faint', 'success', 'danger', 'warning', ...Object.keys(t).filter((name) => name.startsWith('syntax-'))]) {
      for (const background of ['canvas', 'surface', 'subtle', 'hover']) assert.ok(contrast(t[name], t[background]) >= 4.5, `${name} on ${background}: ${contrast(t[name], t[background])}`);
    }
    for (const background of ['diff-add-fill', 'diff-del-fill']) {
      for (const name of ['ink', 'ink-faint', 'success', 'danger', ...Object.keys(t).filter((name) => name.startsWith('syntax-'))]) {
        assert.ok(contrast(t[name], t[background]) >= 4.5, `${name} on ${background}: ${contrast(t[name], t[background])}`);
      }
    }
    for (const background of ['canvas', 'surface', 'subtle', 'hover']) {
      assert.ok(contrast(t['ink-muted'], t[background]) >= 7);
      // Hairline separators stay quiet; control borders meet the 3:1 non-text minimum.
      assert.ok(contrast(t['line-strong'], t[background]) >= 3, `line-strong on ${background}`);
    }
    assert.ok(contrast(t.danger, t['danger-subtle']) >= 4.5);
    assert.ok(contrast(t['inverted-ink'], t.inverted) >= 4.5);
    assert.ok(contrast(t['inverted-ink'], t['inverted-hover']) >= 4.5);
    assert.notEqual(t.success, original.success);
    assert.notEqual(t.danger, original.danger);
  });

  test(`${slug} previews show distinct, visibly coloured accents`, () => {
    const t = tokens(slug);
    const chroma = (hex) => { const c = hex.slice(1).match(/../g).map((v) => parseInt(v, 16)); return (Math.max(...c) - Math.min(...c)) / 255; };
    // The picker chip's four dots and the gallery card's swatch bar (see .tpk-chip and .tp-sw).
    const chip = ['syntax-keyword', 'syntax-string', 'syntax-function', 'warning'].map((name) => t[name]);
    const bar = ['danger', 'warning', 'success', 'syntax-attribute', 'syntax-function', 'syntax-keyword'].map((name) => t[name]);
    assert.equal(new Set(chip).size, chip.length, `chip dots ${chip}`);
    assert.ok(new Set(bar).size >= 5, `swatch bar ${bar}`);
    for (const colour of [...chip, ...bar]) {
      // Readable, but not pushed so far toward black (light) or white (dark) that the hue disappears.
      assert.ok(chroma(colour) >= .25, `${colour} is saturated`);
      assert.ok(contrast(colour, t.surface) <= 8, `${colour} on surface is ${contrast(colour, t.surface).toFixed(2)}:1, not a near-ink tint`);
    }
  });
}

// Full-severity red/green transforms from Machado, Oliveira & Fernandes (2009).
// https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html
// A screening check, not a substitute for feedback from people using the themes.
const redGreenSimulations = {
  protanopia: [[.152286, 1.052583, -.204868], [.114503, .786281, .099216], [-.003882, -.048116, 1.051998]],
  deuteranopia: [[.367322, .860646, -.227968], [.280085, .672501, .047413], [-.011820, .042940, .968881]],
};
function simulate(hex, matrix) {
  const rgb = hex.slice(1).match(/../g).map((c) => parseInt(c, 16) / 255).map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  return '#' + matrix.map((row) => {
    const linear = Math.max(0, Math.min(1, row.reduce((sum, weight, i) => sum + weight * rgb[i], 0)));
    const srgb = linear <= .0031308 ? 12.92 * linear : 1.055 * linear ** (1 / 2.4) - .055;
    return Math.round(srgb * 255).toString(16).padStart(2, '0');
  }).join('');
}
for (const slug of ['colorblind-light', 'colorblind-dark']) {
  for (const [name, matrix] of Object.entries(redGreenSimulations)) {
    test(`${slug} keeps text readable under the ${name} screening transform`, () => {
      const t = Object.fromEntries(Object.entries(tokens(slug)).map(([key, value]) => [key, simulate(value, matrix)]));
      for (const background of ['canvas', 'surface', 'subtle', 'hover', 'diff-add-fill', 'diff-del-fill']) {
        for (const foreground of ['ink', 'ink-faint', 'success', 'danger', 'warning', ...Object.keys(t).filter((key) => key.startsWith('syntax-'))]) {
          assert.ok(contrast(t[foreground], t[background]) >= 4.5, `${foreground} on ${background}: ${contrast(t[foreground], t[background])}`);
        }
      }
    });
  }
}


test('Colorblind picker filter includes both accessible variants and excludes System and ordinary themes', () => {
  const { matchesFilter } = picker();
  const options = [
    { value: 'system', dataset: { variant: '' } },
    { value: 'dna-day', dataset: { variant: 'light' } },
    { value: 'dna-dark', dataset: { variant: 'dark' } },
    { value: 'base24-dracula', dataset: { variant: 'dark' } },
    { value: 'colorblind-light', dataset: { variant: 'light' } },
    { value: 'colorblind-dark', dataset: { variant: 'dark' } },
  ];
  const shown = (filter) => options.filter((option) => matchesFilter(option, filter)).map((option) => option.value);
  assert.deepEqual(shown('colorblind'), ['colorblind-light', 'colorblind-dark']);
  assert.equal(shown('').length, options.length);
  assert.deepEqual(shown('dark'), ['system', 'dna-dark', 'base24-dracula', 'colorblind-dark']);
  assert.deepEqual(shown('light'), ['system', 'dna-day', 'colorblind-light']);
});
