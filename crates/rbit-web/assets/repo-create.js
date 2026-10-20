// Keep form nodes in this page; switching types never serializes credentials or drafts.
const drafts = new WeakMap();
let request = null;
const commonNames = ['owner', 'name', 'description', 'visibility'];
function readCommon(panel, common) {
  for (const name of commonNames) {
    const fields = [...panel.querySelectorAll(`[name="${name}"]`)];
    const field = fields.find(field => field.type !== 'radio' || field.checked);
    if (field) common[name] = field.value;
  }
}
function writeCommon(panel, common) {
  for (const name of commonNames) {
    if (!(name in common)) continue;
    for (const field of panel.querySelectorAll(`[name="${name}"]`)) {
      if (field.type === 'radio') field.checked = field.value === common[name];
      else if (field.tagName !== 'SELECT' || [...field.options].some(option => option.value === common[name])) field.value = common[name];
    }
  }
}
async function switchType(tab) {
  const root = tab.closest('[data-repo-create]');
  const kind = tab.dataset.repoType;
  request?.abort(); request = null;
  if (root.dataset.kind === kind) { root.querySelector('[data-repo-create-status]').textContent = ''; return; }
  let state = drafts.get(root);
  if (!state) {
    state = { panels: new Map([[root.dataset.kind, root.querySelector('[data-repo-create-panel]')]]), common: {} };
    drafts.set(root, state);
  }
  const status = root.querySelector('[data-repo-create-status]');
  let panel = state.panels.get(kind);
  if (!panel) {
    const controller = new AbortController(); request = controller;
    status.textContent = 'Loading form…';
    try {
      const response = await fetch(tab.href, { signal: controller.signal, headers: { Accept: 'text/html' } });
      if (!response.ok || response.redirected) throw new Error('Form unavailable');
      const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
      if (request !== controller || !root.isConnected) return;
      panel = doc.querySelector('[data-repo-create-panel]');
      if (!panel) throw new Error('Form missing');
      panel.hidden = true; root.append(panel); state.panels.set(kind, panel);
      window.htmx?.process(panel);
    } catch (error) {
      if (error.name !== 'AbortError') status.textContent = 'Could not load this form. Select the tab to try again.';
      return;
    }
  }
  // Read at the time of the switch, so edits made during a fetch also carry over.
  readCommon(state.panels.get(root.dataset.kind), state.common);
  writeCommon(panel, state.common);
  for (const [key, content] of state.panels) content.hidden = key !== kind;
  for (const button of root.querySelectorAll('[data-repo-type]')) {
    const selected = button.dataset.repoType === kind;
    button.setAttribute('aria-selected', String(selected)); button.tabIndex = selected ? 0 : -1;
  }
  root.dataset.kind = kind; status.textContent = '';
}
document.addEventListener('click', event => {
  const tab = event.target.closest?.('[data-repo-type]');
  if (!tab || event.button > 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  event.preventDefault(); switchType(tab);
});
document.addEventListener('keydown', event => {
  const tab = event.target.closest?.('[data-repo-type]');
  if (tab && event.key === ' ') { event.preventDefault(); switchType(tab); return; }
  if (!tab || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  const tabs = [...tab.closest('[role=tablist]').querySelectorAll('[data-repo-type]')];
  const index = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (tabs.indexOf(tab) + (event.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length;
  tabs[index].focus(); switchType(tabs[index]);
});
document.addEventListener('htmx:beforeSwap', () => { request?.abort(); request = null; });
