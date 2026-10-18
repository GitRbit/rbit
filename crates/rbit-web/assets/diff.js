// Diff pages (repo/file_diffs.html: commits and compares). Progressive enhancement: without
// it the file tree is a plain list of links and each file still collapses on its own.
//
// * Sticky offset: file headers and the tree stick right below the sticky header, at
//   `--sticky-top` (app.css), which follows its height as app.js measures it.
// * File headers: collapsing a stuck header keeps it where it was instead of leaving the
//   reader somewhere in the next file; Alt-click and "Collapse all" toggle every file.
// * File tree: folders collapse, the filter narrows it by path (every word must match),
//   the file being read is marked, and following a link opens a collapsed file.
// * The tree can be hidden; that choice is remembered per browser.
//
// Listeners are delegated on `document`, so the module works across htmx swaps.

const SIDEBAR_KEY = "rbit.diff-sidebar";
const wide = matchMedia("(width >= 64rem)");

/** Where file headers stick, in viewport pixels. */
function diffTop(layout) {
  return parseFloat(getComputedStyle(layout).getPropertyValue("--diff-top")) || 0;
}

const layoutOf = (element) => element.closest("[data-diff-layout]");

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
  for (const file of layout.querySelectorAll(".diff-file")) {
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
  const button = layout.querySelector("[data-diff-collapse-all]");
  if (!button) return;
  const expanded = [...layout.querySelectorAll(".diff-file")].some((file) => file.open);
  button.dataset.state = expanded ? "expanded" : "collapsed";
  for (const part of button.querySelectorAll("[data-when]")) {
    part.classList.toggle("hidden", part.dataset.when !== button.dataset.state);
    part.classList.toggle("contents", part.dataset.when === button.dataset.state);
  }
}

// ---------------------------------------------------------------- the current file

/** The file whose header is at (or stuck to) the top of the diffs. */
function currentFile(layout) {
  const files = layout.querySelectorAll(".diff-file");
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
  const tree = layout.querySelector("[data-diff-tree]");
  if (!tree) return;
  const file = currentFile(layout);
  const item = file && tree.querySelector(`[data-diff-target="${CSS.escape(file.id)}"]`);
  const previous = tree.querySelector('[aria-current="true"]');
  if (item === previous) return;
  previous?.removeAttribute("aria-current");
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
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    for (const layout of document.querySelectorAll("[data-diff-layout]")) markCurrent(layout);
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
  for (const layout of document.querySelectorAll("[data-diff-layout]")) {
    if (wide.matches) layout.querySelector(".diff-sidebar")?.setAttribute("open", "");
    syncSidebarToggle(layout);
  }
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
      return;
    }
    const file = event.target instanceof HTMLDetailsElement && event.target.matches(".diff-file") ? event.target : null;
    if (file) syncCollapseAll(layoutOf(file));
  },
  true,
);

// ---------------------------------------------------------------- set-up

function init() {
  const hidden = readSidebarPreference() === "hidden";
  for (const layout of document.querySelectorAll("[data-diff-layout]:not([data-ready])")) {
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
  scheduleMark();
}

init();
document.addEventListener("htmx:afterSettle", init);
document.addEventListener("htmx:historyRestore", init);
