// Diff pages (commits, compares and pull request files). Progressive enhancement: without
// it the file tree is a plain list of links and each file still collapses on its own.
//
// * Sticky offset: file headers and the tree stick right below the sticky header, at
//   `--sticky-top` (app.css), which follows its height as app.js measures it.
// * File headers: collapsing a stuck header keeps it where it was instead of leaving the
//   reader somewhere in the next file; Alt-click and "Collapse all" toggle every file.
// * File tree: folders collapse, the filter narrows it by path (every word must match),
//   the file being read is marked, and following a link opens a collapsed file.
// * The tree can be hidden; that choice is remembered per browser.
// * Line comments (pull request files the viewer may comment on): see below.
//
// Listeners are delegated on `document`, so the module works across htmx swaps.

const SIDEBAR_KEY = "rbit.diff-sidebar";
const wide = matchMedia("(width >= 64rem)");

/** Where file headers stick, in viewport pixels. */
function diffTop(layout) {
  // Custom properties can remain calc(...) strings. The file's scroll margin uses
  // the same offset, resolved by the browser to pixels, including fractional ones.
  const file = indexOf(layout).files[0];
  return file ? parseFloat(getComputedStyle(file).scrollMarginTop) || 0 : 0;
}

const layoutOf = (element) => element.closest("[data-diff-layout]");

// Index each mounted diff once. Scrolling must not traverse thousands of code rows
// just to find the file containers and sidebar links. htmx swaps invalidate the index.
const indexes = new WeakMap();
let layouts = [];
const pendingCollapse = new Set();

function indexOf(layout) {
  let index = indexes.get(layout);
  if (index) return index;
  const tree = layout.querySelector("[data-diff-tree]");
  const button = layout.querySelector("[data-diff-collapse-all]");
  index = {
    files: [...layout.querySelectorAll(".diff-file")],
    tree,
    links: new Map([...(tree?.querySelectorAll("[data-diff-target]") ?? [])].map((link) => [link.dataset.diffTarget, link])),
    current: tree?.querySelector('[aria-current="true"]') ?? null,
    button,
    parts: [...(button?.querySelectorAll("[data-when]") ?? [])],
  };
  indexes.set(layout, index);
  return index;
}

// ---------------------------------------------------------------- collapsing files

/** Collapses or expands `file`; a header that was stuck stays at the top of the diffs. */
function setOpen(file, open) {
  if (file.open === open) return;
  const top = diffTop(layoutOf(file));
  const stuck = file.getBoundingClientRect().top < top - 1;
  file.open = open;
  if (stuck) scrollBy({ top: file.getBoundingClientRect().top - top, behavior: "instant" });
}

function setAllOpen(layout, open) {
  const anchor = currentFile(layout);
  for (const file of indexOf(layout).files) {
    if (file !== anchor) file.open = open;
  }
  // Keep the file being read in place while everything around it changes height.
  if (anchor) {
    anchor.open = open;
    if (anchor.getBoundingClientRect().top < diffTop(layout)) anchor.scrollIntoView({ block: "start", behavior: "instant" });
  }
  syncCollapseAll(layout);
}

function syncCollapseAll(layout) {
  const { button, files, parts } = indexOf(layout);
  if (!button) return;
  const state = files.some((file) => file.open) ? "expanded" : "collapsed";
  if (button.dataset.state === state) return;
  button.dataset.state = state;
  for (const part of parts) {
    part.classList.toggle("hidden", part.dataset.when !== button.dataset.state);
    part.classList.toggle("contents", part.dataset.when === button.dataset.state);
  }
}

// ---------------------------------------------------------------- the current file

/** The file whose header is at (or stuck to) the top of the diffs. */
function currentFile(layout) {
  const { files } = indexOf(layout);
  const line = diffTop(layout) + 1;
  // Files are in document order, so the last one starting above the line is current.
  let low = 0;
  let high = files.length - 1;
  let found = null;
  while (low <= high) {
    const middle = (low + high) >> 1;
    if (files[middle].getBoundingClientRect().top <= line) {
      found = files[middle];
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return found ?? files[0] ?? null;
}

function markCurrent(layout) {
  const index = indexOf(layout);
  const { tree } = index;
  if (!tree || (wide.matches ? layout.dataset.sidebar === "hidden" : !tree.closest(".diff-sidebar")?.open)) return;
  const file = currentFile(layout);
  const item = index.links.get(file?.id) ?? null;
  const previous = index.current;
  if (item === previous) return;
  previous?.removeAttribute("aria-current");
  index.current = item;
  if (!item) return;
  item.setAttribute("aria-current", "true");
  // Scroll only the tree, never the page.
  const row = item.getBoundingClientRect();
  const box = tree.getBoundingClientRect();
  if (row.top < box.top) tree.scrollTop -= box.top - row.top + 8;
  else if (row.bottom > box.bottom) tree.scrollTop += row.bottom - box.bottom + 8;
}

let frame = 0;
function scheduleMark() {
  if (frame || !layouts.length) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    for (const layout of pendingCollapse) {
      if (layout.isConnected) syncCollapseAll(layout);
    }
    pendingCollapse.clear();
    for (const layout of layouts) {
      if (layout.isConnected) markCurrent(layout);
    }
  });
}

addEventListener("scroll", scheduleMark, { passive: true });
addEventListener("resize", scheduleMark, { passive: true });

// ---------------------------------------------------------------- the file tree

/** Whether a strict ancestor folder of `path` is in `folders`. */
function hasAncestorIn(path, folders) {
  for (let i = path.lastIndexOf("/"); i > 0; i = path.lastIndexOf("/", i - 1)) {
    if (folders.has(path.slice(0, i))) return true;
  }
  return false;
}

/** Shows the rows that match the filter, or, without one, those not in a collapsed folder. */
function refreshTree(layout) {
  const tree = layout.querySelector("[data-diff-tree]");
  const input = layout.querySelector("[data-diff-filter]");
  if (!tree || !input) return;
  const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
  const rows = tree.querySelectorAll(".diff-tree-row");

  const collapsed = new Set();
  for (const row of rows) {
    if ("folder" in row.dataset && row.firstElementChild.getAttribute("aria-expanded") === "false") collapsed.add(row.dataset.path);
  }

  const shownFolders = new Set();
  let total = 0;
  let shown = 0;
  for (const row of rows) {
    if (!("file" in row.dataset)) continue;
    total += 1;
    const path = row.dataset.path;
    if (words.length) {
      const text = path.toLowerCase();
      const matches = words.every((word) => text.includes(word));
      row.hidden = !matches;
      if (!matches) continue;
      shown += 1;
      for (let i = path.lastIndexOf("/"); i > 0; i = path.lastIndexOf("/", i - 1)) shownFolders.add(path.slice(0, i));
    } else {
      row.hidden = hasAncestorIn(path, collapsed);
      shown += 1;
    }
  }
  for (const row of rows) {
    if (!("folder" in row.dataset)) continue;
    row.hidden = words.length ? !shownFolders.has(row.dataset.path) : hasAncestorIn(row.dataset.path, collapsed);
  }

  tree.querySelector("[data-diff-filter-empty]")?.toggleAttribute("hidden", shown > 0);
  const count = layout.querySelector("[data-diff-filter-count]");
  if (count) count.textContent = words.length ? `${shown}/${total}` : `${total}`;
}

document.addEventListener("input", (event) => {
  const input = event.target instanceof HTMLInputElement && event.target.matches("[data-diff-filter]") ? event.target : null;
  if (input) refreshTree(layoutOf(input));
});

document.addEventListener("keydown", (event) => {
  const input = event.target instanceof HTMLInputElement && event.target.matches("[data-diff-filter]") ? event.target : null;
  if (!input) return;
  if (event.key === "Escape" && input.value) {
    event.preventDefault();
    input.value = "";
    refreshTree(layoutOf(input));
  } else if (event.key === "Enter") {
    // Jump to the first file left by the filter.
    event.preventDefault();
    layoutOf(input).querySelector(".diff-tree-row[data-file]:not([hidden]) a")?.click();
  }
});

// ---------------------------------------------------------------- the tree's visibility

function readSidebarPreference() {
  try {
    return localStorage.getItem(SIDEBAR_KEY);
  } catch {
    return null;
  }
}

function writeSidebarPreference(value) {
  try {
    if (value) localStorage.setItem(SIDEBAR_KEY, value);
    else localStorage.removeItem(SIDEBAR_KEY);
  } catch {
    // Storage blocked: the choice lasts for this page only.
  }
}

function syncSidebarToggle(layout) {
  const button = layout.querySelector("[data-diff-sidebar-toggle]");
  if (!button) return;
  const visible = wide.matches ? layout.dataset.sidebar !== "hidden" : layout.querySelector(".diff-sidebar")?.open;
  button.setAttribute("aria-pressed", String(visible));
}

function toggleSidebar(layout) {
  if (wide.matches) {
    const hide = layout.dataset.sidebar !== "hidden";
    if (hide) layout.dataset.sidebar = "hidden";
    else delete layout.dataset.sidebar;
    writeSidebarPreference(hide ? "hidden" : null);
  } else {
    const sidebar = layout.querySelector(".diff-sidebar");
    sidebar.open = !sidebar.open;
  }
  syncSidebarToggle(layout);
  scheduleMark();
}

wide.addEventListener("change", () => {
  for (const layout of layouts) {
    if (wide.matches) layout.querySelector(".diff-sidebar")?.setAttribute("open", "");
    syncSidebarToggle(layout);
  }
  scheduleMark();
});

// ---------------------------------------------------------------- clicks

document.addEventListener("click", (event) => {
  const target = event.target instanceof Element ? event.target : null;
  const layout = target && layoutOf(target);
  if (!layout) return;

  // Buttons inside a file header act on their own, without toggling the file.
  const header = target.closest(".diff-file-header");
  if (header && target.closest("button, a")) {
    if (target.closest("button")) event.preventDefault();
    return;
  }
  if (header) {
    event.preventDefault();
    const file = header.parentElement;
    if (event.altKey) setAllOpen(layout, !file.open);
    else setOpen(file, !file.open);
    syncCollapseAll(layout);
    return;
  }

  if (target.closest("[data-diff-collapse-all]")) {
    setAllOpen(layout, layout.querySelector("[data-diff-collapse-all]").dataset.state === "collapsed");
    return;
  }

  if (target.closest("[data-diff-sidebar-toggle]")) {
    toggleSidebar(layout);
    return;
  }

  const folder = target.closest(".diff-tree-row[data-folder] > button");
  if (folder) {
    folder.setAttribute("aria-expanded", String(folder.getAttribute("aria-expanded") === "false"));
    refreshTree(layout);
    return;
  }

  const link = target.closest("[data-diff-target]");
  if (link) {
    const file = document.getElementById(link.dataset.diffTarget);
    if (file && !file.open) file.open = true;
    syncCollapseAll(layout);
    // The browser scrolls to the anchor; on a small screen the opened tree closes first.
    if (!wide.matches && layout.querySelector(".diff-sidebar")?.open) {
      layout.querySelector(".diff-sidebar").open = false;
      syncSidebarToggle(layout);
    }
  }
});

// Keyboard toggling of a header (Enter/Space on <summary>) goes through `click` too, but
// the browser's own `toggle` also fires for find-in-page and anchor reveals.
document.addEventListener(
  "toggle",
  (event) => {
    if (event.target instanceof HTMLDetailsElement && event.target.matches(".diff-sidebar")) {
      syncSidebarToggle(layoutOf(event.target));
      scheduleMark();
      return;
    }
    const file = event.target instanceof HTMLDetailsElement && event.target.matches(".diff-file") ? event.target : null;
    const layout = file && layoutOf(file);
    if (layout) {
      pendingCollapse.add(layout);
      scheduleMark();
    }
  },
  true,
);

// ---------------------------------------------------------------- line comments
//
// A pull request file the viewer may comment on carries its Files changed URL
// (`data-comment` on its `.diff-file`); its rows are the plain rows of every other diff, so a
// long diff adds no links, listeners or attributes per line. Clicking either number of a
// line opens the inline form there (`?comment=<row id>`), swapping just the file as the
// server's own links do. One number per file is focusable at a time (a roving tab stop):
// the arrow keys move it between lines, Enter or Space opens the form. Without scripts,
// each hunk header links to the form at its first line.

const LINE_ROW = /-[LR](\d+)$/;
const COMMENT_ROWS = ".diff-add[id], .diff-del[id], .diff-ctx[id]";

/** The line row of a commentable diff that `cell`, one of its numbers, belongs to. */
function commentRow(cell) {
  if (!(cell instanceof Element) || !cell.classList.contains("num")) return null;
  const row = cell.parentElement;
  return row && LINE_ROW.test(row.id) && row.closest("[data-comment]") ? row : null;
}

function openComment(row) {
  const file = row.closest("[data-comment]");
  if (!file) return;
  const url = `${file.dataset.comment}?comment=${encodeURIComponent(row.id)}#${row.id}`;
  const htmx = globalThis.htmx;
  if (!htmx) {
    location.assign(url);
    return;
  }
  const target = `#${file.id}`;
  htmx.ajax("GET", url, { source: row, target, select: target, swap: "outerHTML" });
}

/** The next (or previous) line row of the same diff, across blocks, threads and hunks. */
function adjacentRow(row, forward) {
  const step = (node) => (forward ? node.nextElementSibling : node.previousElementSibling);
  let node = row;
  for (;;) {
    let next = step(node);
    if (!next) {
      // The end of a block: continue after it; the end of the diff: stop.
      const parent = node.parentElement;
      if (!parent || !parent.classList.contains("vblock")) return null;
      node = parent;
      continue;
    }
    if (next.classList.contains("vblock")) {
      const edge = forward ? next.firstElementChild : next.lastElementChild;
      if (!edge) {
        node = next;
        continue;
      }
      next = edge;
    }
    if (LINE_ROW.test(next.id)) return next;
    node = next;
  }
}

/** Makes `row`'s first number its file's one tab stop, releasing `previous`. */
function makeCurrent(row, previous = null) {
  const cell = row.firstElementChild;
  if (previous && previous !== cell) {
    for (const name of ["tabindex", "role", "aria-label"]) previous.removeAttribute(name);
  }
  cell.setAttribute("tabindex", "0");
  cell.setAttribute("role", "button");
  cell.setAttribute("aria-label", `Comment on line ${LINE_ROW.exec(row.id)[1]}`);
  return cell;
}

/** Gives each new commentable file of the mounted diffs its first tab stop. */
function initComments() {
  for (const layout of layouts) {
    for (const file of indexOf(layout).files) {
      if (!("comment" in file.dataset) || "commentReady" in file.dataset) continue;
      file.dataset.commentReady = "";
      // The first match ends the search: the rest of a long diff is not visited.
      const first = file.querySelector(COMMENT_ROWS);
      if (first) makeCurrent(first);
    }
  }
}

document.addEventListener("click", (event) => {
  const row = commentRow(event.target);
  if (!row) return;
  event.preventDefault();
  openComment(row);
});

document.addEventListener("keydown", (event) => {
  const row = commentRow(event.target);
  if (!row || event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    openComment(row);
    return;
  }
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const next = adjacentRow(row, event.key === "ArrowDown");
  if (!next) return;
  event.preventDefault();
  makeCurrent(next, event.target).focus();
});

// ---------------------------------------------------------------- set-up

/**
 * Mounts the diffs in `root` and refreshes every mounted one. A page load or restore
 * searches the document; a swap searches only what it swapped in, so a small fragment
 * elsewhere on the page costs no document-wide query.
 */
function mount(root) {
  const found = root instanceof Element && root.matches("[data-diff-layout]") ? [root] : root.querySelectorAll("[data-diff-layout]");
  if (root === document) {
    layouts = [...found];
  } else {
    layouts = layouts.filter((layout) => layout.isConnected);
    for (const layout of found) if (!layouts.includes(layout)) layouts.push(layout);
  }
  const hidden = readSidebarPreference() === "hidden";
  for (const layout of layouts) {
    indexOf(layout);
    if (layout.hasAttribute("data-ready")) continue;
    layout.dataset.ready = "";
    const sidebar = layout.querySelector(".diff-sidebar");
    if (sidebar) sidebar.open = wide.matches;
    if (hidden) layout.dataset.sidebar = "hidden";
    for (const control of layout.querySelectorAll("[data-diff-sidebar-toggle], [data-diff-collapse-all]")) control.hidden = false;
    syncSidebarToggle(layout);
    syncCollapseAll(layout);
    refreshTree(layout);
    // A link to a file that a restored page kept collapsed opens it.
    const linked = location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (linked instanceof HTMLDetailsElement && layout.contains(linked)) linked.open = true;
  }
  initComments();
  scheduleMark();
}

mount(document);
// A fragment can replace files inside an existing layout. Refresh mounted indexes at
// the next settle, regardless of which element initiated the request.
document.addEventListener("htmx:afterSwap", () => {
  for (const layout of layouts) indexes.delete(layout);
});
// Release strong references before htmx removes a page, including full-body swaps.
document.addEventListener("htmx:beforeCleanupElement", (event) => {
  const root = event.detail.elt;
  if (layouts.length) layouts = layouts.filter((layout) => {
    if (root !== layout && !root.contains(layout)) return true;
    pendingCollapse.delete(layout);
    indexes.delete(layout);
    return false;
  });
  if (!layouts.length && frame) {
    cancelAnimationFrame(frame);
    frame = 0;
  }
});
document.addEventListener("htmx:afterSettle", (event) => mount(event.detail?.elt ?? document));
document.addEventListener("htmx:historyRestore", () => mount(document));
