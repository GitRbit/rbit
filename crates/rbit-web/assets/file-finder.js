// "Go to file" above a repository's files: lists matching paths as you type.
// Without this script the box is a form that opens the server-rendered find page.
// Scoring mirrors `file_match_score` in crates/rbit-web/src/views/file_finder.rs.

const MAX_RESULTS = 20;
const pathLists = new Map(); // paths URL -> Promise<string[]>

function loadPaths(url) {
  if (!pathLists.has(url)) {
    const request = fetch(url, { headers: { Accept: 'text/plain' } })
      .then(response => {
        if (!response.ok) throw new Error('Request failed');
        return response.text();
      })
      .then(text => text.split('\n').filter(Boolean));
    request.catch(() => pathLists.delete(url));
    pathLists.set(url, request);
  }
  return pathLists.get(url);
}

const BOUNDARY = new Set(['/', '_', '-', '.', ' ']);

function matchFrom(hay, needle, start, nameStart) {
  let total = 0;
  let next = start;
  let previous = -2;
  for (const wanted of needle) {
    const found = hay.indexOf(wanted, next);
    if (found < 0) return null;
    total += 1;
    if (previous + 1 === found) total += 5;
    if (found === 0 || BOUNDARY.has(hay[found - 1])) total += 4;
    if (found >= nameStart) total += 2;
    previous = found;
    next = found + 1;
  }
  return total;
}

const needleOf = query => [...query.toLowerCase()].filter(c => !/\s/.test(c));

// `{ score, start }`: the match over the whole path or the file name alone, whichever is
// better, and where its greedy match began; `null` when the path does not match.
export function score(path, query) {
  const needle = needleOf(query);
  if (needle.length === 0) return { score: 0, start: 0 };
  const hay = [...path.toLowerCase()];
  const nameStart = hay.lastIndexOf('/') + 1;
  const whole = matchFrom(hay, needle, 0, nameStart);
  if (whole === null) return null;
  const name = matchFrom(hay, needle, nameStart, nameStart);
  const best = name !== null && name > whole ? { value: name, start: nameStart } : { value: whole, start: 0 };
  return { score: best.value * 16 - hay.length, start: best.start };
}

function encodePath(path) {
  return path.split('/').map(encodeURIComponent).join('/');
}

// Highlights the matched characters of `path` (same greedy rule as `score`).
function highlighted(path, query, start) {
  const fragment = document.createDocumentFragment();
  const needle = needleOf(query);
  const chars = [...path];
  let index = 0;
  let position = 0;
  let run = '';
  let runMatched = false;
  const flush = () => {
    if (!run) return;
    if (runMatched) {
      const mark = document.createElement('mark');
      mark.className = 'file-finder-match';
      mark.textContent = run;
      fragment.append(mark);
    } else {
      fragment.append(run);
    }
    run = '';
  };
  for (const char of chars) {
    const matched = position++ >= start && index < needle.length && char.toLowerCase() === needle[index];
    if (matched) index += 1;
    if (matched !== runMatched) {
      flush();
      runMatched = matched;
    }
    run += char;
  }
  flush();
  return fragment;
}

function setup(form) {
  if (form.fileFinder) return;
  form.fileFinder = true;
  const input = form.querySelector('input[name="q"]');
  const list = form.querySelector('[role="listbox"]');
  let active = -1;
  let generation = 0;

  const options = () => [...list.querySelectorAll('[role="option"]')];
  const close = () => {
    list.hidden = true;
    input.setAttribute('aria-expanded', 'false');
    input.removeAttribute('aria-activedescendant');
    active = -1;
  };
  const highlight = index => {
    const items = options();
    if (items.length === 0) return;
    active = (index + items.length) % items.length;
    items.forEach((item, i) => item.setAttribute('aria-selected', String(i === active)));
    input.setAttribute('aria-activedescendant', items[active].id);
    items[active].scrollIntoView({ block: 'nearest' });
  };
  const message = text => {
    const item = document.createElement('li');
    item.className = 'px-3 py-2 text-[13px] text-ink-muted';
    item.textContent = text;
    list.replaceChildren(item);
    list.hidden = false;
    input.setAttribute('aria-expanded', 'true');
  };

  async function update() {
    const query = input.value.trim();
    const current = ++generation;
    if (!query) return close();
    let paths;
    try {
      if (!pathLists.has(form.dataset.pathsUrl)) message(form.dataset.loading);
      paths = await loadPaths(form.dataset.pathsUrl);
    } catch {
      if (current === generation) message(form.dataset.failed);
      return;
    }
    if (current !== generation) return;
    const matches = [];
    for (const path of paths) {
      const match = score(path, query);
      if (match !== null) matches.push([match.score, path, match.start]);
    }
    matches.sort((a, b) => b[0] - a[0] || (a[1] < b[1] ? -1 : 1));
    if (matches.length === 0) return message(form.dataset.noMatch);
    const items = matches.slice(0, MAX_RESULTS).map(([, path, start], index) => {
      const item = document.createElement('li');
      item.setAttribute('role', 'option');
      item.id = `file-finder-option-${index}`;
      const link = document.createElement('a');
      link.className = 'menu-item font-mono text-xs';
      link.dir = 'ltr';
      link.href = form.dataset.blobBase + encodePath(path);
      link.tabIndex = -1;
      const name = document.createElement('span');
      name.className = 'truncate';
      name.append(highlighted(path, query, start));
      link.append(name);
      item.append(link);
      return item;
    });
    if (matches.length > MAX_RESULTS) {
      const more = document.createElement('li');
      const link = document.createElement('a');
      link.className = 'menu-item text-xs text-ink-muted';
      link.href = `${form.action}?q=${encodeURIComponent(query)}`;
      link.tabIndex = -1;
      link.textContent = `${form.dataset.seeAll} (${matches.length})`;
      more.append(link);
      items.push(more);
    }
    list.replaceChildren(...items);
    list.hidden = false;
    input.setAttribute('aria-expanded', 'true');
    highlight(0);
  }

  input.addEventListener('focus', () => {
    loadPaths(form.dataset.pathsUrl).catch(() => {});
    if (input.value.trim()) update();
  });
  input.addEventListener('input', update);
  input.addEventListener('keydown', event => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (list.hidden) return;
      event.preventDefault();
      highlight(active + (event.key === 'ArrowDown' ? 1 : -1));
    } else if (event.key === 'Enter' && !list.hidden && active >= 0) {
      const link = options()[active]?.querySelector('a');
      if (!link) return;
      event.preventDefault();
      link.click();
    } else if (event.key === 'Escape') {
      if (!list.hidden) {
        event.preventDefault();
        close();
      } else {
        input.blur();
      }
    }
  });
  form.addEventListener('focusout', event => {
    if (!form.contains(event.relatedTarget)) close();
  });
  // Keep focus in the box while a result is clicked.
  list.addEventListener('mousedown', event => event.preventDefault());
}

document.addEventListener('focusin', event => {
  const form = event.target.closest?.('[data-file-finder]');
  if (form) setup(form);
});

// `t` focuses the box, as on other code hosts, unless the viewer is typing.
document.addEventListener('keydown', event => {
  if (event.key !== 't' || event.ctrlKey || event.metaKey || event.altKey || event.defaultPrevented) return;
  const target = event.target;
  if (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
  const input = document.querySelector('[data-file-finder] input[name="q"]');
  if (!input) return;
  event.preventDefault();
  input.focus();
});
