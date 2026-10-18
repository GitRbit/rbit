// Language controls open a dialog without navigating away from the repository.
let languageRequest = null;

function closeLanguageBrowser() {
  languageRequest?.abort();
  languageRequest = null;
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
  dialog.setAttribute('aria-label', 'Language files');
  const loading = document.createElement('p');
  loading.className = 'p-6 text-ink-muted';
  loading.setAttribute('role', 'status');
  loading.textContent = 'Loading files…';
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'btn m-4';
  close.dataset.languageClose = '';
  close.textContent = 'Close';
  dialog.append(loading, close);
  dialog.addEventListener('close', () => {
    controller.abort();
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
    content.querySelector('[data-language-close]').hidden = false;
    dialog.setAttribute('aria-labelledby', 'language-files-title');
    window.htmx?.process(content);
    content.querySelector('[data-language-close]').focus();
  } catch (error) {
    if (error.name === 'AbortError') return;
    loading.textContent = 'Unable to load files. ';
    const retry = document.createElement('button');
    retry.type = 'button';
    retry.className = 'link';
    retry.textContent = 'Try again';
    retry.addEventListener('click', () => showLanguageFiles(trigger, url));
    loading.append(retry);
  }
}

document.addEventListener('click', event => {
  const target = event.target instanceof Element ? event.target : null;
  const trigger = target?.closest('[data-language-files]');
  if (trigger) {
    event.preventDefault();
    // htmx's boosted click handler does not check defaultPrevented.
    event.stopPropagation();
    showLanguageFiles(trigger);
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
