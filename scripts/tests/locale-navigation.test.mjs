import assert from 'node:assert/strict';
import test from 'node:test';
import { installLocale } from '../../crates/rbit-web/assets/locale.js';
import { catalogKey } from '../../crates/rbit-web/assets/locale-key.js';

function fixture(initial = '1.en', saved = initial) {
  const listeners = new Map();
  const windowListeners = new Map();
  const storage = new Map([['rbit-catalog-history', saved], ['htmx-history-cache', 'old HTML'], ['theme', 'dark']]);
  const removed = [];
  let reloads = 0;
  let documentKey = initial;
  let restoredKey = initial;
  const document = {
    documentElement: { getAttribute: () => documentKey },
    querySelector: () => ({ getAttribute: () => restoredKey }),
    addEventListener: (name, callback) => listeners.set(name, callback),
  };
  const window = {
    location: { href: 'https://forge.example/settings/language', origin: 'https://forge.example', reload: () => reloads++ },
    localStorage: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, value),
      removeItem: (key) => { removed.push(key); storage.delete(key); },
    },
    addEventListener: (name, callback) => windowListeners.set(name, callback),
  };
  installLocale(document, window);
  const dispatch = (name, detail) => listeners.get(name)?.({ detail });
  return {
    dispatch, storage, removed, reloads: () => reloads,
    restore: (key) => { restoredKey = key; dispatch('htmx:historyRestore', {}); },
    pageshow: () => windowListeners.get('pageshow')({ persisted: true }),
    swap: (headers = {}, extra = {}) => {
      const values = new Headers({ 'Content-Type': 'text/html', 'Rbit-Catalog-Key': '1.es', ...headers });
      const detail = {
        shouldSwap: true, requestConfig: { verb: 'get' },
        xhr: { status: 200, responseURL: window.location.href, getResponseHeader: (name) => values.get(name) }, ...extra,
      };
      dispatch('htmx:beforeSwap', detail);
      return detail;
    },
  };
}

test('catalog identities are bounded closed values, never arbitrary cache namespaces', () => {
  for (const key of ['1.en', '2.es', '1.zh-Hans', '1.pt-BR', '1.ja', '1.de', '1.ru', '1.fr', '1.ko', '1.en-XA', '1.ar-XB']) assert.equal(catalogKey(key), key);
  for (const key of ['', 'es', '0.en', '1234567.es', '1.es-MX', '1.zh-Hant', '1.zh-hans', '1.pt', '1.en\n', null]) assert.equal(catalogKey(key), null);
});

test('native language navigation invalidates only translated HTML history', () => {
  const app = fixture('1.es', '1.en');
  assert.deepEqual(app.removed, ['htmx-history-cache']);
  assert.equal(app.storage.get('theme'), 'dark');
  assert.equal(app.storage.get('rbit-catalog-history'), '1.es');
  assert.deepEqual(fixture().removed, []);
  const detail = { headers: {} };
  app.dispatch('htmx:configRequest', detail);
  assert.equal(detail.headers['Rbit-Document-Locale'], '1.es');
});

test('cross-catalog same-origin HTML cannot swap into stale shell, reload happens once', () => {
  const app = fixture();
  assert.equal(app.swap().shouldSwap, false);
  app.swap();
  assert.equal(app.reloads(), 1);
  const same = fixture('1.es');
  assert.equal(same.swap().shouldSwap, true);
  assert.equal(same.reloads(), 0);
});

test('raw, sandboxed, failed, cross-origin and POST responses never trigger locale reload', () => {
  const cases = [
    [{ 'Content-Type': 'application/json' }, {}],
    [{ 'Content-Disposition': 'attachment' }, {}],
    [{ 'Content-Security-Policy': 'sandbox allow-scripts' }, {}],
    [{}, { isError: true }],
    [{}, { requestConfig: { verb: 'post' } }],
    [{}, { xhr: { status: 404 } }],
    [{}, { xhr: { status: 200, responseURL: 'https://other.example/', getResponseHeader: (name) => name === 'Rbit-Catalog-Key' ? '1.es' : 'text/html' } }],
  ];
  for (const [headers, detail] of cases) {
    const app = fixture();
    app.swap(headers, detail);
    assert.equal(app.reloads(), 0);
  }
});

test('htmx body history and persisted browser pages detect changed catalog without loops', () => {
  const app = fixture();
  app.restore('1.es');
  assert.equal(app.reloads(), 1);
  const persisted = fixture();
  persisted.storage.set('rbit-catalog-history', '1.es');
  persisted.pageshow();
  persisted.pageshow();
  assert.equal(persisted.reloads(), 1);
  const same = fixture();
  same.restore('1.en');
  same.pageshow();
  assert.equal(same.reloads(), 0);
});
