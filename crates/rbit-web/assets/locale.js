import { documentCatalogKey, catalogKey } from './locale-key.js';
import { clearNavigationResponses } from './repo-navigation-cache.js';

const HISTORY = 'htmx-history-cache';
const NAMESPACE = 'rbit-catalog-history';

/** Keep translated HTML history separate without purging HTTP or asset caches. */
export function installLocale(document, window) {
  let reloading = false;
  const stored = () => {
    try { return catalogKey(window.localStorage.getItem(NAMESPACE)); }
    catch { return null; }
  };
  const reload = () => {
    if (reloading) return;
    reloading = true;
    clearNavigationResponses();
    window.location.reload();
  };
  const current = documentCatalogKey(document);
  if (current) {
    try {
      if (stored() !== current) {
        window.localStorage.removeItem(HISTORY);
        window.localStorage.setItem(NAMESPACE, current);
        clearNavigationResponses();
      }
    } catch { /* Storage-disabled browsers still use the response boundary. */ }
  }
  document.addEventListener('htmx:configRequest', ({ detail }) => {
    const key = documentCatalogKey(document);
    if (key && detail.headers) detail.headers['Rbit-Document-Locale'] = key;
  });
  document.addEventListener('htmx:beforeSwap', ({ detail }) => {
    if (!detail.shouldSwap || detail.isError || detail.requestConfig?.verb?.toLowerCase() !== 'get') return;
    const xhr = detail.xhr;
    if (!xhr || xhr.status < 200 || xhr.status >= 300) return;
    const selected = catalogKey(xhr.getResponseHeader('Rbit-Catalog-Key'));
    if (!selected || selected === documentCatalogKey(document)) return;
    if (!/^text\/html(?:\s*;|$)/i.test(xhr.getResponseHeader('Content-Type') ?? '')) return;
    if (xhr.getResponseHeader('Content-Disposition') || /^sandbox\b/.test(xhr.getResponseHeader('Content-Security-Policy') ?? '')) return;
    try {
      if (new URL(xhr.responseURL, window.location.href).origin !== window.location.origin) return;
    } catch { return; }
    detail.shouldSwap = false;
    reload();
  });
  document.addEventListener('htmx:historyRestore', () => {
    // htmx restores body HTML, while the document element survives the swap.
    const restored = catalogKey(document.querySelector('.site-header')?.getAttribute('data-document-catalog'));
    if (restored && restored !== documentCatalogKey(document)) reload();
  });
  window.addEventListener('pageshow', (event) => {
    if (event.persisted && stored() && stored() !== documentCatalogKey(document)) reload();
  });
}

if (typeof document !== 'undefined' && typeof window !== 'undefined') installLocale(document, window);
