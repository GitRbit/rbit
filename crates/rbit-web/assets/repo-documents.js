// Repository documents use normal links as a fallback. Only one document is
// mounted at once, keeping Markdown anchors and diagram instances unambiguous.
let pendingDocument = null;
const outlines = new WeakMap();
const MAX_PAGE_CHARACTERS = 4 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 20000;
const documents = new Map();
const loads = new Map();
const MAX_CACHED_DOCUMENTS = 8;
const MAX_CACHE_CHARACTERS = 4 * 1024 * 1024;
let activeSection = null;
let warmingSection = null;
let generation = 0;

// The element htmx swapped in (or the content an event names); the document otherwise.
const rootOf = (event) => {
  const root = event?.detail?.root ?? event?.detail?.elt ?? event?.target;
  return typeof root?.querySelectorAll === 'function' ? root : document;
};
/** Elements in `root` matching `selector`, `root` included. */
const within = (root, selector) => {
  const found = [...root.querySelectorAll(selector)];
  if (root.matches?.(selector)) found.unshift(root);
  return found;
};

function documentURL(link) {
  const url = new URL(link.href, location.href);
  url.hash = '';
  url.searchParams.set('document_fragment', 'true');
  return url;
}

function rememberDocument(key, panel) {
  const size = panel.outerHTML.length;
  if (size > MAX_CACHE_CHARACTERS) return;
  documents.delete(key);
  // Keep a pristine template: live diagrams and other enhancements can mutate a
  // mounted panel, and detached iframes lose their browsing context.
  documents.set(key, { panel: panel.cloneNode(true), size });
  let total = [...documents.values()].reduce((sum, entry) => sum + entry.size, 0);
  while (documents.size > MAX_CACHED_DOCUMENTS || total > MAX_CACHE_CHARACTERS) {
    const oldest = documents.keys().next().value;
    total -= documents.get(oldest).size;
    documents.delete(oldest);
  }
}

function clearDocuments() {
  generation++;
  visibilityObserver?.disconnect();
  cancelDocumentRequest();
  for (const load of loads.values()) load.controller.abort();
  loads.clear();
  documents.clear();
  warmingSection = null;
}

function cancelDocumentRequest() {
  if (!pendingDocument) return;
  pendingDocument.load.controller?.abort();
  if (loads.get(pendingDocument.key) === pendingDocument.load) loads.delete(pendingDocument.key);
  pendingDocument.section.removeAttribute('aria-busy');
  const status = pendingDocument.section.querySelector('[data-document-status]');
  if (status) status.hidden = true;
  pendingDocument = null;
}

/** Marks the links to `section`'s documents in `root` (the whole page by default). */
function updateDocumentLinks(section, root = document) {
  for (const link of within(root, '[data-repo-document]')) {
    const active = link.dataset.repoDocument === section.dataset.documentKey;
    if (link.getAttribute('role') === 'tab') {
      link.setAttribute('aria-selected', String(active));
      link.tabIndex = active ? 0 : -1;
    } else if (active) link.setAttribute('aria-current', 'true');
    else link.removeAttribute('aria-current');
  }
}

function createOutline(section) {
  if (outlines.has(section)) return;
  const list = section.querySelector('[data-document-headings]');
  const menu = section.querySelector('[data-document-outline]');
  if (!list || !menu) return;
  // Reuse the application's outside-click and navigation menu dismissal.
  menu.dataset.menu = '';
  // History snapshots contain old menu entries; rebuild their element references.
  list.replaceChildren();
  const headings = Array.from(section.querySelectorAll('[data-document-panel] :is(h1,h2,h3,h4,h5,h6)')).slice(0, 500);
  const minimum = headings.reduce((level, heading) => Math.min(level, Number(heading.tagName.slice(1))), 6);
  const entries = [];
  for (const heading of headings) {
    const label = heading.textContent.trim();
    if (!label) continue;
    const preceding = heading.previousElementSibling;
    const target = heading.id ? heading : preceding?.matches('a[id]') ? preceding : heading;
    if (!target.id) {
      let id = `repo-document-heading-${entries.length + 1}`;
      while (document.getElementById(id)) id += '-section';
      target.id = id;
    }
    const item = document.createElement('li');
    item.dataset.headingDepth = String(Math.max(0, Math.min(5, Number(heading.tagName.slice(1)) - minimum)));
    const link = document.createElement('a');
    const url = new URL(location.href);
    url.hash = target.id;
    link.href = url.href;
    link.textContent = label;
    link.dataset.documentHeading = String(entries.length);
    link.setAttribute('hx-boost', 'false');
    item.append(link);
    list.append(item);
    entries.push({ target, item, link, text: label.toLocaleLowerCase() });
  }
  outlines.set(section, entries);
  menu.hidden = entries.length === 0;
  const filter = section.querySelector('[data-document-heading-filter]');
  if (filter) filter.value = '';
  section.querySelector('[data-document-no-headings]').hidden = true;
}

function refreshHeadingLinks(section) {
  for (const entry of outlines.get(section) ?? []) {
    const url = new URL(location.href);
    url.hash = entry.target.id;
    entry.link.href = url.href;
  }
}

/**
 * Sets up the documents sections in `root` (the whole page by default): new content
 * from a swap or an inserted panel. Sections elsewhere on the page are unchanged.
 */
function initializeDocuments(root = document) {
  if (activeSection && !activeSection.isConnected) {
    clearDocuments();
    activeSection = null;
  }
  const sections = within(root, '[data-repo-documents]');
  for (const section of sections) {
    if (activeSection !== section) {
      clearDocuments();
      activeSection = section;
      const selected = section.querySelector('[role="tab"][aria-selected="true"]');
      const panel = section.querySelector('[data-document-panel]');
      if (selected && panel) rememberDocument(documentURL(selected).href, panel);
      // Warm a small, bounded set only once the reader approaches the documents.
      visibilityObserver?.observe(section);
    }
    createOutline(section);
    updateDocumentLinks(section);
    // A history snapshot may have been taken while a request was loading.
    if (pendingDocument?.section !== section) {
      section.removeAttribute('aria-busy');
      const status = section.querySelector('[data-document-status]');
      if (status) status.hidden = true;
    }
  }
  // Links to the documents may also arrive on their own (outside any section).
  if (activeSection && !sections.includes(activeSection)) updateDocumentLinks(activeSection, root);
}

function loadDocument(link, section, speculative = false) {
  const url = documentURL(link);
  const key = url.href;
  const cached = documents.get(key);
  if (cached) {
    documents.delete(key);
    documents.set(key, cached);
    return { key, promise: Promise.resolve(cached.panel.cloneNode(true)) };
  }
  if (loads.has(key)) return loads.get(key);
  const controller = new AbortController();
  const started = generation;
  const load = { key, controller, timer: null, promise: null };
  load.timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  const scope = section.dataset.documentScope;
  load.promise = (async () => {
    const response = await fetch(url.href, { signal: controller.signal, credentials: 'same-origin', priority: speculative ? 'low' : 'high', headers: { Accept: 'text/html' } });
    if (!response.ok || Number(response.headers?.get('content-length')) > MAX_PAGE_CHARACTERS) throw new Error('Document unavailable');
    if (response.url) {
      const final = new URL(response.url, location.href);
      if (final.origin !== url.origin || final.pathname !== url.pathname || final.search !== url.search) throw new Error('Document redirected');
    }
    const html = await response.text();
    if (html.length > MAX_PAGE_CHARACTERS) throw new Error('Document page too large');
    if (controller.signal.aborted) throw new Error('Document request timed out');
    const replacement = new DOMParser().parseFromString(html, 'text/html').querySelector('[data-repo-documents]');
    const panel = replacement?.querySelector('[data-document-panel]');
    if (!panel || replacement.dataset.documentKey !== link.dataset.repoDocument || replacement.dataset.documentScope !== scope) throw new Error('Missing document');
    if (started === generation && activeSection === section && section.isConnected) rememberDocument(key, panel);
    return panel;
  })().finally(() => {
    clearTimeout(load.timer);
    if (loads.get(key) === load) loads.delete(key);
  });
  loads.set(key, load);
  return load;
}

function canWarmDocuments() {
  const connection = globalThis.navigator?.connection;
  return !connection?.saveData && !['slow-2g', '2g'].includes(connection?.effectiveType);
}

function warmDocument(link) {
  const section = document.querySelector('[data-repo-documents]');
  if (!section || !canWarmDocuments() || loads.size >= 2 || link.dataset.repoDocument === section.dataset.documentKey) return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin || !url.pathname.startsWith(`${section.dataset.documentScope}/`)) return;
  if (documents.has(documentURL(link).href)) return;
  loadDocument(link, section, true).promise.catch(() => {});
}

async function warmDocuments(section) {
  if (warmingSection === section || !canWarmDocuments()) return;
  warmingSection = section;
  const started = generation;
  // Sequential, low-priority reads avoid competing with the page and Git requests.
  const tabs = [...section.querySelectorAll('[role="tab"][data-repo-document]')].filter(link => link.dataset.repoDocument !== section.dataset.documentKey).slice(0, 6);
  for (const tab of tabs) {
    if (started !== generation || activeSection !== section || !section.isConnected || pendingDocument || loads.size >= 2) break;
    await loadDocument(tab, section, true).promise.catch(() => {});
  }
}

const visibilityObserver = typeof IntersectionObserver === 'undefined' ? null : new IntersectionObserver(entries => {
  for (const entry of entries) if (entry.isIntersecting) {
    visibilityObserver.unobserve(entry.target);
    warmDocuments(entry.target);
  }
}, { rootMargin: '300px' });

for (const eventName of ['pointerover', 'focusin', 'touchstart']) document.addEventListener(eventName, event => {
  if (!(event.target instanceof Element)) return;
  const link = event.target.closest('[data-repo-document]');
  if (link) warmDocument(link);
}, { passive: true });

function documentError(section, url) {
  const status = section.querySelector('[data-document-status]');
  status.replaceChildren(document.createTextNode('Unable to load this document. Your current document is still available. '));
  const fallback = document.createElement('a');
  fallback.href = url.href;
  fallback.textContent = (globalThis.rbitMessages?.text("browser-open-document") ?? (globalThis.rbitMessages?.text("browser-open-document") ?? "Open the document page"));
  fallback.setAttribute('hx-boost', 'false');
  status.append(fallback);
  status.hidden = false;
}

function scrollToDocument(section, hash = '#readme') {
  let id;
  try { id = decodeURIComponent(hash.slice(1)); } catch { id = ''; }
  const target = [...section.querySelectorAll('[id]')].find(element => element.id === id) ?? section;
  target.scrollIntoView({ block: 'start', behavior: 'instant' });
}

async function switchDocument(link, { restore = false, focus = false } = {}) {
  const section = document.querySelector('[data-repo-documents]');
  if (!section) return;
  const url = new URL(link.href, location.href);
  const scope = section.dataset.documentScope;
  if (url.origin !== location.origin || !scope || !url.pathname.startsWith(`${scope}/`)) return;
  cancelDocumentRequest();
  if (link.dataset.repoDocument === section.dataset.documentKey) {
    if (!restore) history.replaceState(history.state, '', url.href);
    refreshHeadingLinks(section);
    section.querySelector('[role="tab"][aria-selected="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    if (restore || link.getAttribute('role') !== 'tab') scrollToDocument(section, restore ? location.hash : url.hash);
    return;
  }
  const stickyTop = Number.parseFloat(getComputedStyle(section.querySelector('.repo-documents-header')).top) || 0;
  const shouldScroll = restore || link.getAttribute('role') !== 'tab' || section.getBoundingClientRect().top <= stickyTop + 1;
  const load = loadDocument(link, section);
  const request = { load, key: load.key, section };
  pendingDocument = request;
  section.setAttribute('aria-busy', 'true');
  const status = section.querySelector('[data-document-status]');
  status.hidden = true;
  try {
    const panel = await load.promise;
    if (pendingDocument !== request || !section.isConnected) return;
    section.querySelector('[data-document-panel]').replaceWith(panel);
    section.dataset.documentKey = link.dataset.repoDocument;
    section.querySelector('[data-document-outline]').open = false;
    outlines.delete(section);
    pendingDocument = null;
    section.removeAttribute('aria-busy');
    if (!restore) history.replaceState(history.state, '', url.href);
    initializeDocuments(section);
    document.dispatchEvent(new CustomEvent('rbit:content-loaded', { detail: { root: panel } }));
    const selected = section.querySelector('[role="tab"][aria-selected="true"]');
    selected?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    if (focus) selected?.focus({ preventScroll: true });
    if (shouldScroll) scrollToDocument(section, restore ? location.hash : url.hash);
  } catch (error) {
    // An older response or navigation must never affect the current page.
    if (pendingDocument === request && section.isConnected) documentError(section, url);
  } finally {
    if (pendingDocument === request) {
      pendingDocument = null;
      section.removeAttribute('aria-busy');
    }
  }
}

// Capture before boosted navigation sees any document or heading link.
document.addEventListener('click', (event) => {
  if (!(event.target instanceof Element) || (event.button ?? 0) !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const heading = event.target.closest('[data-document-heading]');
  const link = event.target.closest('[data-repo-document]');
  if (!heading && !link) return;
  event.preventDefault();
  event.stopPropagation();
  if (heading) {
    const section = heading.closest('[data-repo-documents]');
    const entry = outlines.get(section)?.[Number(heading.dataset.documentHeading)];
    if (!entry) return;
    const destination = new URL(location.href);
    destination.hash = entry.target.id;
    history.replaceState(history.state, '', destination.href);
    section.querySelector('[data-document-outline]').open = false;
    entry.target.scrollIntoView({ block: 'start', behavior: 'instant' });
    // Do not steal focus back to the sticky menu after it closes.
    entry.target.setAttribute('tabindex', '-1');
    entry.target.focus({ preventScroll: true });
  } else switchDocument(link, { focus: link.getAttribute('role') === 'tab' });
}, true);

document.addEventListener('keydown', (event) => {
  if (!(event.target instanceof Element)) return;
  const tab = event.target.closest('[role="tab"][data-repo-document]');
  if (tab && ['ArrowLeft', 'ArrowRight', 'Home', 'End', ' ', 'Enter'].includes(event.key)) {
    const tabs = [...tab.closest('[role="tablist"]').querySelectorAll('[role="tab"]')];
    let index = tabs.indexOf(tab);
    if (event.key === 'ArrowLeft') index = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === 'ArrowRight') index = (index + 1) % tabs.length;
    else if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = tabs.length - 1;
    event.preventDefault();
    if (event.key === ' ' || event.key === 'Enter') switchDocument(tab, { focus: true });
    else {
      for (const item of tabs) item.tabIndex = item === tabs[index] ? 0 : -1;
      tabs[index].focus();
      tabs[index].scrollIntoView({ block: 'nearest', inline: 'nearest' });
    }
  }
  if (event.key === 'Escape') {
    const menu = event.target.closest('[data-document-outline]');
    if (menu?.open) { menu.open = false; menu.querySelector('summary').focus(); }
  }
});

document.addEventListener('input', (event) => {
  if (!(event.target instanceof Element) || !event.target.matches('[data-document-heading-filter]')) return;
  const section = event.target.closest('[data-repo-documents]');
  const terms = event.target.value.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  let visible = 0;
  for (const entry of outlines.get(section) ?? []) {
    entry.item.hidden = !terms.every(term => entry.text.includes(term));
    if (!entry.item.hidden) visible++;
  }
  section.querySelector('[data-document-no-headings]').hidden = visible > 0;
});

document.addEventListener('htmx:beforeRequest', (event) => {
  const verb = event.detail?.requestConfig?.verb;
  if (event.detail?.boosted || event.detail?.requestConfig?.boosted || (verb && verb.toLowerCase() !== 'get')) clearDocuments();
});
addEventListener('popstate', clearDocuments);
addEventListener('pagehide', clearDocuments);
document.addEventListener('htmx:historyCacheMiss', clearDocuments);
document.addEventListener('htmx:afterSettle', (event) => initializeDocuments(rootOf(event)));
document.addEventListener('rbit:content-loaded', (event) => initializeDocuments(rootOf(event)));
document.addEventListener('htmx:historyRestore', () => {
  clearDocuments();
  initializeDocuments();
  // htmx may restore a snapshot captured before a replaceState tab switch.
  const section = document.querySelector('[data-repo-documents]');
  if (!section) return;
  const requested = new URL(location.href).searchParams.get('document');
  const tabs = [...section.querySelectorAll('[role="tab"][data-repo-document]')];
  const desired = requested ? tabs.find(tab => new URL(tab.href, location.href).searchParams.get('document') === requested) : tabs[0];
  if (desired && desired.dataset.repoDocument !== section.dataset.documentKey) switchDocument(desired, { restore: true });
});
initializeDocuments();
