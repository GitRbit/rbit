// Keep repository chrome mounted while the server supplies fresh navigation state.
// Content is reconciled from trusted SSR nodes, never interpreted as HTML strings.
const TRANSPORT_ATTRIBUTES = new Set(['hx-swap-oob', 'data-hx-swap-oob']);

function nodeKey(node) {
  if (node.nodeType !== 1) return null;
  const tab = node.getAttribute('data-repo-tab');
  if (tab) return `tab:${tab}`;
  if (node.id) return `id:${node.id}`;
  for (const name of ['repo-tabs', 'repo-title', 'repo-actions']) {
    if (node.classList.contains(name)) return `part:${name}`;
  }
  return null;
}

function compatible(current, incoming) {
  return current.nodeType === incoming.nodeType && current.nodeName === incoming.nodeName && nodeKey(current) === nodeKey(incoming);
}

function affectsHtmx(node, name) {
  return name.startsWith('hx-') || name.startsWith('data-hx-') ||
    (node.tagName === 'A' && name === 'href') ||
    (node.tagName === 'FORM' && ['action', 'method'].includes(name));
}

function synchronize(current, incoming, process) {
  if (current.nodeType !== 1) {
    if (current.nodeValue !== incoming.nodeValue) current.nodeValue = incoming.nodeValue;
    return;
  }
  const input = current.tagName === 'INPUT';
  const valueChanged = input && current.value !== incoming.value;
  const selected = valueChanged && current.ownerDocument.activeElement === current;
  const selection = selected ? [current.selectionStart, current.selectionEnd, current.selectionDirection] : null;
  // A clone menu is reader state. Updating counts or the selected repository tab
  // must not close it, move its cursor, or replace its copy buttons.
  const preserveOpen = current.tagName === 'DETAILS';
  const wanted = new Map([...incoming.attributes].map(attribute => [attribute.name, attribute.value]));
  for (const attribute of [...current.attributes]) {
    const name = attribute.name;
    if (preserveOpen && name === 'open') continue;
    if (TRANSPORT_ATTRIBUTES.has(name) || !wanted.has(name)) {
      current.removeAttribute(name);
      if (!TRANSPORT_ATTRIBUTES.has(name) && affectsHtmx(current, name)) process.add(current);
    }
  }
  for (const [name, value] of wanted) {
    if (TRANSPORT_ATTRIBUTES.has(name) || (preserveOpen && name === 'open')) continue;
    if (current.getAttribute(name) !== value) {
      current.setAttribute(name, value);
      if (affectsHtmx(current, name)) process.add(current);
    }
  }
  if (input && current.value !== incoming.value) current.value = incoming.value;
  reconcileChildren(current, incoming, process);
  if (selection && selection[0] !== null && typeof current.setSelectionRange === 'function') {
    // Only text inputs support a selection; hidden star form fields do not.
    current.setSelectionRange(Math.min(selection[0], current.value.length), Math.min(selection[1], current.value.length), selection[2]);
  }
}

function reconcileChildren(current, incoming, process) {
  const remaining = [...current.childNodes];
  const wanted = [...incoming.childNodes];
  let cursor = current.firstChild;
  for (const next of wanted) {
    const key = nodeKey(next);
    let match = key ? remaining.find(node => nodeKey(node) === key && compatible(node, next)) :
      remaining.find(node => compatible(node, next));
    if (match) {
      remaining.splice(remaining.indexOf(match), 1);
      if (match !== cursor) current.insertBefore(match, cursor);
      synchronize(match, next, process);
    } else {
      match = next.cloneNode(true);
      current.insertBefore(match, cursor);
      if (match.nodeType === 1) process.add(match);
    }
    cursor = match.nextSibling;
  }
  for (const node of remaining) node.remove();
}

/** Update one repository's navigation without remounting tabs, actions or menus. */
export function reconcileRepositoryBar(current, incoming) {
  if (!current || !incoming || current.nodeType !== 1 || incoming.nodeType !== 1 || current.id !== 'repo-subnav' || incoming.id !== 'repo-subnav') return false;
  const scope = current.getAttribute('data-repo-scope');
  if (!scope || !scope.startsWith('/') || scope !== incoming.getAttribute('data-repo-scope')) return false;
  const currentTabs = current.querySelector('.repo-tabs');
  const incomingTabs = incoming.querySelector('.repo-tabs');
  if (!currentTabs || !incomingTabs) return false;
  const keys = [...incomingTabs.querySelectorAll('[data-repo-tab]')].map(tab => tab.getAttribute('data-repo-tab'));
  if (!keys.length || new Set(keys).size !== keys.length) return false;
  const focused = current.contains(current.ownerDocument.activeElement) ? current.ownerDocument.activeElement : null;
  const scroll = [current.scrollLeft, current.scrollTop, currentTabs.scrollLeft, currentTabs.scrollTop];
  const process = new Set();
  synchronize(current, incoming, process);
  // Process only changed bindings and newly inserted controls. Stable anchors
  // keep their existing htmx and preload listeners without duplicate wiring.
  for (const node of process) {
    if (current.contains(node) && ![...process].some(parent => parent !== node && parent.contains(node))) globalThis.htmx?.process(node);
  }
  if (focused && current.contains(focused) && current.ownerDocument.activeElement !== focused) focused.focus({ preventScroll: true });
  [current.scrollLeft, current.scrollTop, currentTabs.scrollLeft, currentTabs.scrollTop] = scroll;
  return true;
}
