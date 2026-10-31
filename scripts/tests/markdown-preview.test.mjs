import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/rbit-web/assets/markdown-preview.js', import.meta.url), 'utf8');

function mount({ file = false, failure = false, reply = false, pathValue = 'docs/README.md', translations = {} } = {}) {
  const handlers = {}, requests = [], created = [];
  let release;
  class Element {
    constructor() { this.dataset = {}; this.attributes = {}; this.listeners = {}; this.children = []; }
    setAttribute(key, value) { this.attributes[key] = value; }
    addEventListener(name, callback) { this.listeners[name] = callback; }
    append(...children) { this.children.push(...children); }
    focus() { this.focused = true; }
    click() { return this.listeners.click(); }
    remove() { this.removed = true; }
    matches(selector) { return selector === '[data-markdown-preview-panel]' && 'markdownPreviewPanel' in this.dataset; }
  }
  const path = new Element();
  path.value = pathValue;
  const input = new Element();
  Object.assign(input, { id: reply ? 'reviewcomment-4-reply' : 'body', name: file ? 'content' : 'body', value: '**Draft**', form: { elements: { path } },
    before(element) { this.previousElementSibling = element; }, after(element) { this.nextElementSibling = element; } });
  if (file) input.dataset.previewUrl = '/issues/markdown-preview?owner=alice&repo=demo&revision=feature%2Fdocs';
  const document = { querySelectorAll(selector) {
    // A shared review reply has no explicit editor opt-in; only its stable name/id.
    if (reply && !(selector.includes('[name="body"]') && selector.includes('[id$="-reply"]'))) return [];
    return [input];
  }, addEventListener(name, callback) { handlers[name] = callback; }, createElement() { const element = new Element(); created.push(element); return element; } };
  const window = { htmx: { ajax(method, url, options) { requests.push({ method, url, options }); return new Promise((resolve, reject) => { release = failure ? () => reject(new Error('offline')) : resolve; }); } } };
  vm.runInNewContext(source, { document, window, rbitMessages: { text: key => translations[key] }, location: { pathname: '/alice/demo/issues/1' }, URLSearchParams });
  return { input, path, handlers, requests, created, release: () => release(), write: created[2], preview: created[3], panel: created[1], tabs: created[0] };
}

test('preview posts only the current draft with repository context and Write preserves it', async () => {
  const page = mount();
  assert.equal(page.panel.hidden, true);
  const pending = page.preview.click();
  assert.equal(page.input.hidden, true);
  assert.equal(page.panel.hidden, false);
  assert.equal(page.requests[0].url, '/issues/markdown-preview?owner=alice&repo=demo');
  assert.equal(page.requests[0].method, 'POST');
  assert.equal(JSON.stringify(page.requests[0].options.values), '{"body":"**Draft**"}');
  assert.equal(page.panel.attributes['aria-busy'], 'true');
  await page.preview.click();
  assert.equal(page.requests.length, 1, 'a pending preview cannot be submitted twice');
  page.input.previousElementSibling.listeners.keydown({ key: 'ArrowLeft', preventDefault() {} });
  assert.equal(page.input.hidden, false);
  assert.equal(page.input.value, '**Draft**');
  assert.equal(page.panel.hidden, true);
  page.release(); await pending;
  assert.equal(page.panel.hidden, true, 'a late response does not switch the selected tab');
  assert.equal(page.preview.attributes['aria-disabled'], 'false');
  const next = page.preview.click();
  page.input.listeners.invalid();
  assert.equal(page.input.hidden, false, 'native validation can focus the source');
  page.release(); await next;
});

test('file preview posts the current path and content at the server supplied revision', async () => {
  const page = mount({ file: true });
  const pending = page.preview.click();
  assert.match(page.requests[0].url, /revision=feature%2Fdocs/);
  assert.equal(JSON.stringify(page.requests[0].options.values), '{"content":"**Draft**","path":"docs/README.md"}');
  page.release(); await pending;
});

test('failed requests leave an accessible message and source stays available', async () => {
  const page = mount({ failure: true });
  const pending = page.preview.click();
  page.release(); await pending;
  assert.match(page.panel.textContent, /could not be loaded/);
  assert.equal(page.panel.attributes['aria-busy'], 'false');
  page.write.click();
  assert.equal(page.input.value, '**Draft**');
  page.handlers['htmx:responseError']({ detail: { target: page.panel } });
  assert.match(page.panel.textContent, /could not be loaded/);
});

test('htmx swaps mount once and history restores working tabs with the source intact', () => {
  const page = mount();
  page.handlers['htmx:afterSettle']();
  assert.equal(page.created.length, 4);
  const oldTabs = page.input.previousElementSibling;
  page.handlers['htmx:historyRestore']();
  assert.equal(oldTabs.removed, true);
  assert.equal(page.created.length, 8);
  assert.equal(page.input.value, '**Draft**');
  let prevented = false;
  page.input.previousElementSibling.listeners.keydown({ key: 'Home', preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(page.input.hidden, false);
});

test('shared review replies without an editor attribute get previews and restore working tabs', async () => {
  const page = mount({ reply: true });
  assert.equal('markdownEditor' in page.input.dataset, false);
  assert.equal(page.created.length, 4);
  const pending = page.preview.click();
  assert.equal(JSON.stringify(page.requests[0].options.values), '{"body":"**Draft**"}');
  page.release(); await pending;
  const oldTabs = page.input.previousElementSibling;
  page.handlers['htmx:historyRestore']();
  assert.equal(oldTabs.removed, true);
  assert.equal(page.created.length, 8, 'the shared reply selector also applies on history restore');
  assert.equal(page.input.value, '**Draft**');
});

test('new file and plain-text paths enable previews only for the four Markdown extensions', async () => {
  const page = mount({ file: true, pathValue: '' });
  assert.equal(page.tabs.hidden, true);
  assert.equal(page.input.hidden, false);
  await page.preview.click();
  assert.equal(page.requests.length, 0);
  for (const path of ['README.md', 'docs/Guide.MARKDOWN', 'notes.mdown', 'notes.mkd']) {
    page.path.value = path;
    page.path.listeners.input();
    assert.equal(page.tabs.hidden, false, path);
    assert.equal(page.input.value, '**Draft**', 'a filename change keeps the draft');
  }
  for (const path of ['notes.txt', 'notes.md.js', 'notes.mdx', '']) {
    page.path.value = path;
    page.path.listeners.input();
    assert.equal(page.tabs.hidden, true, path);
    assert.equal(page.input.hidden, false);
  }
  page.path.value = 'new/docs/README.md';
  page.path.listeners.input();
  const pending = page.preview.click();
  assert.equal(page.requests[0].options.values.path, 'new/docs/README.md');
  page.release(); await pending;
});

test('renaming a file closes stale previews and keeps late responses hidden for plain files', async () => {
  const page = mount({ file: true });
  const pending = page.preview.click();
  assert.equal(page.panel.hidden, false);
  page.path.value = 'moved/guide.txt';
  page.path.listeners.input();
  assert.equal(page.tabs.hidden, true);
  assert.equal(page.input.hidden, false);
  assert.equal(page.panel.hidden, true);
  page.release(); await pending;
  assert.equal(page.panel.hidden, true);
  assert.equal(page.input.value, '**Draft**');
  page.path.value = 'moved/guide.md';
  page.path.listeners.input();
  assert.equal(page.tabs.hidden, false);
  const second = page.preview.click();
  assert.equal(page.requests[1].options.values.path, 'moved/guide.md');
  page.path.value = 'another/guide.md';
  page.path.listeners.input();
  assert.equal(page.input.hidden, false, 'moving a Markdown file also invalidates its old document base');
  assert.equal(page.panel.hidden, true);
  page.release(); await second;
});


test('browser catalog supplies editor controls and error messages as text', async () => {
  const page = mount({ failure: true, translations: {
    'browser-write': 'Écrire', 'browser-preview': 'Aperçu', 'browser-preview-loading': 'Chargement…',
    'browser-preview-failed': '<strong>Échec</strong>', 'browser-markdown-editor': 'Éditeur Markdown',
  } });
  assert.equal(page.write.textContent, 'Écrire');
  assert.equal(page.preview.textContent, 'Aperçu');
  assert.equal(page.tabs.attributes['aria-label'], 'Éditeur Markdown');
  const pending = page.preview.click();
  assert.equal(page.panel.textContent, 'Chargement…');
  page.release(); await pending;
  assert.equal(page.panel.textContent, '<strong>Échec</strong>', 'translation remains text rather than markup');
});
