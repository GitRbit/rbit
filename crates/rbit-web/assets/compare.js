// Fetch a bounded set of choices from the selected repository. Text input also works alone.
const searches = new WeakMap();
function field(input) { return input.closest('[data-compare-field]'); }
function close(input) {
  field(input).querySelector('[role=listbox]').hidden = true;
  input.setAttribute('aria-expanded', 'false');
  input.removeAttribute('aria-activedescendant');
}
function cancel(input) {
  const state = searches.get(input);
  clearTimeout(state?.timer);
  state?.controller?.abort();
  searches.delete(input);
}
async function search(input, state) {
  const form = input.closest('[data-compare-picker]');
  const repo = form.elements[`${input.name}_repo`]?.value || form.dataset.repository;
  const status = field(input).querySelector('[data-compare-status]');
  const list = field(input).querySelector('[role=listbox]');
  // Repository values come from editable fields. Encode each path part before fetching.
  const parts = repo.split('/');
  if (parts.length !== 2 || parts.some(part => !part || part === '.' || part === '..')) return;
  const url = `/${parts.map(encodeURIComponent).join('/')}/compare/refs?${new URLSearchParams({ q: state.query ?? input.value, branches: String(form.dataset.branchesOnly === 'true') })}`;
  status.textContent = 'Finding branches and tags…';
  try {
    const response = await fetch(url, { signal: state.controller.signal, headers: { Accept: 'application/json' } });
    if (!response.ok) throw new Error('Search failed');
    const options = await response.json();
    if (searches.get(input) !== state || !input.isConnected) return;
    list.replaceChildren();
    for (const [index, option] of options.slice(0, 50).entries()) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'menu-item font-mono text-xs';
      button.id = `${list.id}-${index}`;
      button.setAttribute('role', 'option');
      button.setAttribute('aria-selected', 'false');
      button.dataset.compareValue = option.value;
      button.textContent = `${option.label} · ${option.kind}`;
      list.append(button);
    }
    list.hidden = options.length === 0;
    input.setAttribute('aria-expanded', String(options.length > 0));
    status.textContent = options.length === 50 ? 'First 50 matches. Type more to narrow the list.' : options.length ? '' : 'No matches. You can type a revision or commit ID.';
  } catch (error) {
    if (error.name !== 'AbortError' && searches.get(input) === state) status.textContent = 'Search is unavailable. Type a revision and press Compare.';
  }
}
function schedule(input, delay, all = false) {
  cancel(input);
  close(input);
  const state = { controller: new AbortController(), query: all ? "" : input.value };
  searches.set(input, state);
  state.timer = setTimeout(() => search(input, state), delay);
}
function choose(input, button) {
  input.value = button.dataset.compareValue;
  input.focus();
  cancel(input);
  close(input);
}
document.addEventListener('input', event => {
  const input = event.target;
  if (input.matches?.('[data-compare-ref]')) schedule(input, 150);
  else if (input.matches?.('[data-compare-picker] input[list]')) {
    const form = input.closest('form');
    for (const ref of form.querySelectorAll('[data-compare-ref]')) { cancel(ref); close(ref); }
  }
});
document.addEventListener('focusin', event => {
  if (event.target.matches?.('[data-compare-ref]')) schedule(event.target, 0, true);
});
document.addEventListener('focusout', event => {
  const container = event.target.closest?.('[data-compare-field]');
  if (container && !container.contains(event.relatedTarget)) {
    const input = container.querySelector('[data-compare-ref]');
    cancel(input); close(input);
  }
});
document.addEventListener('click', event => {
  const button = event.target.closest?.('[data-compare-value]');
  if (button) choose(field(button).querySelector('[data-compare-ref]'), button);
});
document.addEventListener('keydown', event => {
  const input = event.target;
  if (!input.matches?.('[data-compare-ref]')) return;
  const list = field(input).querySelector('[role=listbox]');
  const options = [...list.children];
  if (event.key === 'Escape') { cancel(input); close(input); return; }
  if (list.hidden || !options.length) return;
  const current = options.findIndex(option => option.id === input.getAttribute('aria-activedescendant'));
  if (event.key === 'Enter' && current >= 0) { event.preventDefault(); choose(input, options[current]); }
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
  event.preventDefault();
  const index = ((current < 0 && event.key === 'ArrowUp' ? 0 : current) + (event.key === 'ArrowDown' ? 1 : options.length - 1) + options.length) % options.length;
  for (const [i, option] of options.entries()) { option.setAttribute('aria-selected', String(i === index)); option.classList.toggle('bg-subtle', i === index); }
  input.setAttribute('aria-activedescendant', options[index].id);
  options[index].scrollIntoView({ block: 'nearest' });
});
document.addEventListener('htmx:beforeSwap', () => {
  for (const input of document.querySelectorAll('[data-compare-ref]')) cancel(input);
});
