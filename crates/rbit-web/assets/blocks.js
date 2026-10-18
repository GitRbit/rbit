// Exact placeholder heights for contained content (app.css "Virtual blocks"): virtual
// blocks (`.vblock`) and open changed files (`.diff-file`), both `data-rows` rows of
// `--row-h` each. Every page works without this file: a placeholder then falls back to a
// guess until the content has been rendered once, and `auto` remembers its real height.
//
// Browsers with typed `attr()` read `data-rows` in CSS; here the same value is set
// through the CSSOM, which the CSP allows (unlike `style=` attributes in the markup).
// Changed files need this in every browser for one thing CSS cannot know: whether the
// diff's horizontal scrollbar takes space, which depends on the platform's scrollbars
// (none take space on phones and macOS) and on whether the diff is wider than the file.

const TYPED_ATTR = CSS.supports(
  "contain-intrinsic-block-size",
  "auto calc(attr(data-rows type(<number>), 128) * 1.25rem)",
);
const SIZED = ".vblock[data-rows], .diff-file[data-rows]";

/** Height in px of a horizontal scrollbar under a diff (0 for overlay scrollbars). */
const scrollbarHeight = (() => {
  let height;
  return () => {
    if (height === undefined) {
      const probe = document.createElement("div");
      probe.className = "diff-scroll";
      probe.style.cssText = "position: absolute; visibility: hidden; overflow-x: scroll; width: 4rem; height: 4rem";
      document.body.append(probe);
      height = probe.offsetHeight - probe.clientHeight;
      probe.remove();
    }
    return height;
  };
})();

/**
 * Whether the diff in `file` scrolls sideways: its minimum width (app.css `.diff-view`,
 * from the widest line, known without laying out the skipped rows) is wider than the file.
 */
function scrollsSideways(file) {
  const view = file.querySelector(":scope > .diff-scroll > .diff-view");
  if (!view) return false;
  // `max(100%, 753.8px)`: the second term is the widest line plus the gutters.
  const widths = getComputedStyle(view).minWidth.match(/[\d.]+(?=px)/g);
  return widths !== null && Math.max(...widths.map(Number)) > file.clientWidth;
}

/**
 * Sizes `elements`: measures them all first, then writes, so writing one never makes the
 * browser lay the page out again to measure the next.
 */
function size(elements) {
  const scrollbar = elements.length ? scrollbarHeight() : 0;
  const heights = elements.map((element) => {
    const rows = Number(element.dataset.rows);
    if (!Number.isInteger(rows) || rows <= 0) return null;
    const extra = scrollbar > 0 && element.matches(".diff-file") && scrollsSideways(element) ? scrollbar : 0;
    if (TYPED_ATTR && extra === 0) return "";
    return `auto calc(${rows} * var(--row-h) + ${extra}px)`;
  });
  elements.forEach((element, i) => {
    if (heights[i] === "") element.style.removeProperty("contain-intrinsic-block-size");
    else if (heights[i] !== null) element.style.containIntrinsicBlockSize = heights[i];
  });
}

// Whether a diff scrolls sideways changes with the width of the column of files (the
// window's, the file tree's). Files rendered since keep the height `auto` remembered.
const widths = new WeakMap();
const observed = new WeakSet();
const columns = new ResizeObserver((entries) => {
  for (const { target, contentRect } of entries) {
    if (widths.get(target) === contentRect.width) continue;
    const resized = widths.has(target);
    widths.set(target, contentRect.width);
    if (resized) size([...target.querySelectorAll(":scope > .diff-file[data-rows]")]);
  }
});

/** Sizes every contained element in `root`, `root` included. */
function sizeAll(root) {
  const elements = [...root.querySelectorAll(SIZED)];
  if (root instanceof Element && root.matches(SIZED)) elements.push(root);
  size(elements);
  for (const element of elements) {
    const column = element.matches(".diff-file") ? element.parentElement : null;
    if (column && !observed.has(column)) {
      observed.add(column);
      columns.observe(column);
    }
  }
}

sizeAll(document);
// New content from any htmx swap, and pages restored from htmx's history cache.
document.addEventListener("htmx:load", (event) => sizeAll(event.detail.elt));
document.addEventListener("htmx:historyRestore", () => sizeAll(document));
