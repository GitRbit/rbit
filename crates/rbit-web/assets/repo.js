// Language controls open a dialog without navigating away from the repository.
let languageRequest = null;
let previewRequest = null;

function indentLanguageFiles(content) {
  for (const row of content.querySelectorAll('[data-depth]')) {
    const depth = Number(row.dataset.depth);
    if (Number.isInteger(depth) && depth >= 0) row.style.setProperty('--depth', String(depth));
  }
}

function closeLanguageBrowser() {
  languageRequest?.abort();
  languageRequest = null;
  previewRequest?.abort();
  previewRequest = null;
  document.querySelector('[data-language-dialog]')?.close();
}

// `url` is a page or search of the listing already shown; it stays in the dialog.
async function showLanguageFiles(trigger, url = trigger.dataset.languageFilesUrl ?? trigger.href) {
  closeLanguageBrowser();
  document.querySelector('[data-language-dialog]')?.remove();
  const controller = new AbortController();
  languageRequest = controller;
  const dialog = document.createElement('dialog');
  dialog.className = 'language-dialog';
  dialog.dataset.languageDialog = '';
  dialog.languageTrigger = trigger;
  dialog.setAttribute('aria-label', (globalThis.rbitMessages?.text("browser-language-files") ?? (globalThis.rbitMessages?.text("browser-language-files") ?? "Language files")));
  const loading = document.createElement('p');
  loading.className = 'p-6 text-ink-muted';
  loading.setAttribute('role', 'status');
  loading.textContent = (globalThis.rbitMessages?.text("browser-files-loading") ?? (globalThis.rbitMessages?.text("browser-files-loading") ?? "Loading files…"));
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'btn m-4';
  close.dataset.languageClose = '';
  close.textContent = (globalThis.rbitMessages?.text("browser-close") ?? (globalThis.rbitMessages?.text("browser-close") ?? "Close"));
  dialog.append(loading, close);
  dialog.addEventListener('close', () => {
    controller.abort();
    previewRequest?.abort();
    dialog.remove();
    if (trigger.isConnected) trigger.focus({ preventScroll: true });
  });
  dialog.addEventListener('click', event => {
    if (event.target !== dialog) return;
    const box = dialog.getBoundingClientRect();
    if (event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom) dialog.close();
  });
  document.body.append(dialog);
  dialog.showModal();
  try {
    // The default also handles language links retained in htmx's history from before this change.
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok) throw new Error('Request failed');
    const page = new DOMParser().parseFromString(await response.text(), 'text/html');
    const content = page.querySelector('[data-language-content]');
    if (!content) throw new Error('Missing file list');
    if (!dialog.open) return;
    dialog.replaceChildren(content);
    indentLanguageFiles(content);
    content.querySelector('[data-language-close]').hidden = false;
    dialog.setAttribute('aria-labelledby', 'language-files-title');
    window.htmx?.process(content);
    content.querySelector('[data-language-close]').focus();
  } catch (error) {
    if (error.name === 'AbortError') return;
    loading.textContent = (globalThis.rbitMessages?.text("browser-files-failed") ?? (globalThis.rbitMessages?.text("browser-files-failed") ?? "Unable to load files. "));
    const retry = document.createElement('button');
    retry.type = 'button';
    retry.className = 'link';
    retry.textContent = (globalThis.rbitMessages?.text("browser-retry") ?? (globalThis.rbitMessages?.text("browser-retry") ?? "Try again"));
    retry.addEventListener('click', () => showLanguageFiles(trigger, url));
    loading.append(retry);
  }
}

// Reuse the bounded, syntax-highlighted blob response, including its binary/LFS
// fallback, rather than downloading unrestricted raw files into the browser.
async function previewLanguageFile(link) {
  const browser = link.closest('[data-language-content]');
  const preview = browser.querySelector('[data-language-preview]');
  previewRequest?.abort();
  const controller = new AbortController();
  previewRequest = controller;
  for (const selected of browser.querySelectorAll('[data-outline-file][aria-current]')) selected.removeAttribute('aria-current');
  link.setAttribute('aria-current', 'true');
  preview.setAttribute('aria-busy', 'true');
  const status = document.createElement('p');
  status.className = 'p-6 text-sm text-ink-muted';
  status.setAttribute('role', 'status');
  status.textContent = (globalThis.rbitMessages?.text("browser-file-loading") ?? (globalThis.rbitMessages?.text("browser-file-loading") ?? "Loading file…"));
  preview.replaceChildren(status);
  try {
    const response = await fetch(link.href, { signal: controller.signal });
    if (!response.ok) throw new Error('Request failed');
    const page = new DOMParser().parseFromString(await response.text(), 'text/html');
    const content = page.querySelector('[data-file-preview]');
    if (!content) throw new Error('Missing file content');
    if (controller.signal.aborted || !preview.isConnected) return;
    const heading = document.createElement('div');
    heading.className = 'language-preview-path';
    const path = document.createElement('strong');
    path.textContent = link.title;
    const full = document.createElement('a');
    full.href = link.href;
    full.className = 'link shrink-0';
    full.textContent = (globalThis.rbitMessages?.text("browser-open-file") ?? (globalThis.rbitMessages?.text("browser-open-file") ?? "Open file"));
    heading.append(path, full);
    // IDs must remain unique alongside the repository page and within previews.
    for (const element of content.querySelectorAll('[id]')) element.removeAttribute('id');
    preview.replaceChildren(heading, content);
    window.htmx?.process(preview);
    preview.dispatchEvent(new Event('rbit:content-loaded', { bubbles: true }));
  } catch (error) {
    if (error.name === 'AbortError') return;
    status.textContent = (globalThis.rbitMessages?.text("browser-file-failed") ?? (globalThis.rbitMessages?.text("browser-file-failed") ?? "Unable to load this file. "));
    const retry = document.createElement('button');
    retry.type = 'button';
    retry.className = 'link';
    retry.textContent = (globalThis.rbitMessages?.text("browser-retry") ?? (globalThis.rbitMessages?.text("browser-retry") ?? "Try again"));
    retry.addEventListener('click', () => previewLanguageFile(link));
    status.append(retry);
  } finally {
    if (!controller.signal.aborted) preview.setAttribute('aria-busy', 'false');
  }
}

document.addEventListener('click', event => {
  const target = event.target instanceof Element ? event.target : null;
  // Preview gutters are local to this browser, not anchors in the repository page.
  if (target?.closest('[data-language-preview] .line-number[data-line]')) {
    event.preventDefault();
    event.stopPropagation();
    return;
  }
  const trigger = target?.closest('[data-language-files]');
  if (trigger) {
    event.preventDefault();
    // htmx's boosted click handler does not check defaultPrevented.
    event.stopPropagation();
    showLanguageFiles(trigger);
    return;
  }
  const file = target?.closest('[data-language-dialog] [data-outline-file]');
  if (file && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) {
    event.preventDefault();
    event.stopPropagation();
    previewLanguageFile(file);
    return;
  }
  const pageLink = target?.closest('[data-language-dialog] [data-listing-page]');
  if (pageLink) {
    event.preventDefault();
    event.stopPropagation();
    showLanguageFiles(pageLink.closest('[data-language-dialog]').languageTrigger, pageLink.href);
    return;
  }
  if (target?.closest('[data-language-close]')) closeLanguageBrowser();
  const folder = target?.closest('[data-outline-folder]');
  if (!folder) return;
  folder.setAttribute('aria-expanded', String(folder.getAttribute('aria-expanded') === 'false'));
  const outline = folder.closest('[data-file-outline]');
  const collapsed = [...outline.querySelectorAll('[aria-expanded="false"]')].map(button => button.parentElement.dataset.path + '/');
  for (const row of outline.querySelectorAll('[data-path]')) {
    row.hidden = collapsed.some(prefix => row.dataset.path.startsWith(prefix));
  }
}, true);

document.addEventListener('submit', event => {
  const form = event.target instanceof Element ? event.target.closest('[data-language-dialog] [data-listing-search]') : null;
  if (!form) return;
  event.preventDefault();
  event.stopPropagation();
  const url = new URL(form.action, location.href);
  url.search = new URLSearchParams(new FormData(form)).toString();
  showLanguageFiles(form.closest('[data-language-dialog]').languageTrigger, url.href);
}, true);

document.addEventListener('htmx:beforeSwap', event => {
  if (event.detail.requestConfig?.boosted && event.detail.shouldSwap) closeLanguageBrowser();
});
addEventListener('popstate', closeLanguageBrowser);

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
function indentLanguageBrowsers(event) {
  for (const content of within(rootOf(event), '[data-language-content]')) indentLanguageFiles(content);
}
indentLanguageBrowsers();
document.addEventListener('htmx:afterSettle', indentLanguageBrowsers);
