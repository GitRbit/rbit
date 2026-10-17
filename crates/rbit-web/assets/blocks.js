// Exact placeholder heights for virtual blocks (`.vblock`, app.css) in browsers without
// typed `attr()`. Every page works without this file: a block then falls back to the
// height of a full block until it has been rendered once.
//
// A block's height is `data-rows` rows of `--row-h` each. Browsers with typed `attr()`
// read that in CSS; here the same value is set through the CSSOM, which the CSP allows
// (unlike `style=` attributes in the markup).

const TYPED_ATTR = CSS.supports(
  "contain-intrinsic-block-size",
  "auto calc(attr(data-rows type(<number>), 128) * 1.25rem)",
);

function sizeBlock(block) {
  const rows = Number(block.dataset.rows);
  if (Number.isInteger(rows) && rows > 0) block.style.containIntrinsicBlockSize = `auto calc(${rows} * var(--row-h))`;
}

/** Sizes every block in `root`, `root` included. */
function sizeBlocks(root) {
  if (root instanceof Element && root.matches(".vblock[data-rows]")) sizeBlock(root);
  for (const block of root.querySelectorAll(".vblock[data-rows]")) sizeBlock(block);
}

if (!TYPED_ATTR) {
  sizeBlocks(document);
  // New content from any htmx swap, and pages restored from htmx's history cache.
  document.addEventListener("htmx:load", (event) => sizeBlocks(event.detail.elt));
  document.addEventListener("htmx:historyRestore", () => sizeBlocks(document));
}
