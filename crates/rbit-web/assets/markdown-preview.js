// Write/Preview tabs keep the source textarea in its form, including while hidden.
// Only the server's sanitized renderer supplies preview HTML; no draft is persisted.
let previewSequence = 0;
// Review replies also exist in shared conversation fragments from the review-thread UI.
const markdownEditors = 'textarea[data-markdown-editor], textarea[name="body"][id$="-reply"]';

function isMarkdownPath(path) {
  return /\.(md|markdown|mdown|mkd)$/i.test(path);
}

function previewContext(input) {
  if (input.dataset.previewUrl) return input.dataset.previewUrl;
  const [, owner, repo] = location.pathname.split('/');
  return `/issues/markdown-preview?${new URLSearchParams({ owner: decodeURIComponent(owner), repo: decodeURIComponent(repo) })}`;
}

function enhanceMarkdownEditors() {
  for (const input of document.querySelectorAll(markdownEditors)) {
    if (input.dataset.previewMounted || !window.htmx) continue;
    input.dataset.previewMounted = 'true';
    const id = `markdown-preview-${++previewSequence}`;
    if (!input.id) input.id = `${id}-source`;
    const tabs = document.createElement('div');
    tabs.className = 'flex gap-1 mt-2';
    tabs.setAttribute('role', 'tablist');
    tabs.setAttribute('aria-label', (globalThis.rbitMessages?.text("browser-markdown-editor") ?? (globalThis.rbitMessages?.text("browser-markdown-editor") ?? "Markdown editor")));
    const panel = document.createElement('div');
    panel.id = id;
    panel.dataset.markdownPreviewPanel = '';
    panel.className = 'prose-rbit box p-3 mt-2 min-h-24';
    panel.hidden = true;
    panel.setAttribute('role', 'tabpanel');
    panel.setAttribute('aria-live', 'polite');
    const buttons = [(globalThis.rbitMessages?.text("browser-write") ?? (globalThis.rbitMessages?.text("browser-write") ?? "Write")), (globalThis.rbitMessages?.text("browser-preview") ?? (globalThis.rbitMessages?.text("browser-preview") ?? "Preview"))].map((label) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'tab h-11 sm:h-8';
      button.textContent = label;
      button.setAttribute('role', 'tab');
      return button;
    });
    const [write, preview] = buttons;
    const path = input.name === 'content' ? input.form.elements.path : null;
    let loading = false;
    write.id = `${id}-write`;
    preview.id = `${id}-tab`;
    write.setAttribute('aria-controls', input.id);
    preview.setAttribute('aria-controls', id);
    panel.setAttribute('aria-labelledby', preview.id);
    function select(showPreview) {
      input.hidden = showPreview;
      panel.hidden = !showPreview;
      buttons.forEach((button, index) => {
        const selected = showPreview === (index === 1);
        button.setAttribute('aria-selected', String(selected));
        button.tabIndex = selected ? 0 : -1;
      });
    }
    select(false);
    if (path) {
      const updatePath = () => {
        // A renamed file has a new document base. Keep its draft, but discard the
        // selected preview until the user requests rendering at the current path.
        select(false);
        tabs.hidden = !isMarkdownPath(path.value);
      };
      path.addEventListener('input', updatePath);
      updatePath();
    }
    // Native required/maxlength validation must be able to focus the source.
    input.addEventListener('invalid', () => select(false));
    write.addEventListener('click', () => { select(false); input.focus(); });
    preview.addEventListener('click', async () => {
      if (loading || (path && !isMarkdownPath(path.value))) return;
      select(true);
      loading = true;
      // Keep the active tab focusable so arrow keys can return to Write while loading.
      preview.setAttribute('aria-disabled', 'true');
      panel.textContent = (globalThis.rbitMessages?.text("browser-preview-loading") ?? (globalThis.rbitMessages?.text("browser-preview-loading") ?? "Loading preview…"));
      panel.setAttribute('aria-busy', 'true');
      const values = { [input.name]: input.value };
      if (path) values.path = path.value;
      try {
        await window.htmx.ajax('POST', previewContext(input), { target: panel, swap: 'innerHTML', values });
      } catch {
        panel.textContent = (globalThis.rbitMessages?.text("browser-preview-failed") ?? (globalThis.rbitMessages?.text("browser-preview-failed") ?? "Preview could not be loaded. Return to Write and try again."));
      } finally {
        loading = false;
        preview.setAttribute('aria-disabled', 'false');
        panel.setAttribute('aria-busy', 'false');
      }
    });
    tabs.addEventListener('keydown', (event) => {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      const button = event.key === 'Home' ? write : event.key === 'End' ? preview : event.target === write ? preview : write;
      button.focus();
      button.click();
    });
    tabs.append(...buttons);
    input.before(tabs);
    input.after(panel);
  }
}

enhanceMarkdownEditors();
document.addEventListener('htmx:responseError', (event) => {
  const panel = event.detail.target;
  if (panel?.matches('[data-markdown-preview-panel]')) {
    panel.textContent = (globalThis.rbitMessages?.text("browser-preview-failed") ?? (globalThis.rbitMessages?.text("browser-preview-failed") ?? "Preview could not be loaded. Return to Write and try again."));
  }
});
document.addEventListener('htmx:afterSettle', enhanceMarkdownEditors);
document.addEventListener('htmx:historyRestore', () => {
  // History serializes the controls but not their event listeners.
  for (const input of document.querySelectorAll(markdownEditors)) {
    if (!input.dataset.previewMounted) continue;
    input.previousElementSibling?.remove();
    input.nextElementSibling?.remove();
    delete input.dataset.previewMounted;
    input.hidden = false;
  }
  enhanceMarkdownEditors();
});
