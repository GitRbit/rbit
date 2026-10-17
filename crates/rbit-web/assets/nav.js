// Navigation speed and polish on top of htmx's boosted links (hx-boost on <body>).
// Every page works without this file; it only makes moving between pages faster.
//
// * Preloading: eligible links get `preload="mouseover always"` for the vendored htmx
//   preload extension (enabled with hx-ext on <body>), so a page starts loading after a
//   ~100 ms hover or on touchstart. The server keeps a preloaded page fresh for a few
//   seconds, so the click is answered from the browser cache.
// * Navigation epoch: every htmx request carries a per-page-load value that changes
//   after any state-changing request. HTML varies on it, so a page preloaded before a
//   form post is never served after it.
// * Partial swaps: between pages of the same repository only <main> and the repository
//   tabs are replaced; the top bar stays mounted.
// * Progress bar after a short delay (CSS), and focus moved to <main> after navigating.
// * Render time: the footer's "rendered in" figure follows every htmx request, partial
//   swaps included, from the response's `Server-Timing` header (see `timing.rs`).

// Title-Case like any HTTP header name (the server lists it in `Vary` with that spelling).
const EPOCH_HEADER = "Rbit-Nav-Epoch";
const newEpoch = () => `${Date.now().toString(36)}.${Math.random().toString(36).slice(2, 8)}`;
let epoch = newEpoch();

// Mirrors `middleware::is_unsafe_to_preload` (the server's answer is authoritative; this
// only avoids sending requests that would be refused).
const NO_PRELOAD = [
  /^\/logout(\/|$)/,
  /^\/settings\/security/,
  /^\/login\/two-factor/,
  /^\/email\/verify\//,
  /^\/password\/reset\//,
  /^\/(api|assets)\//,
  /^\/sw\.js$/,
  /^\/(healthz|readyz|metrics)$/,
  /^\/[^/]+\/[^/]+\/(raw|archive|info|git-upload-pack|git-receive-pack)(\/|$)/,
  /^\/[^/]+\/[^/]+\/releases\/download\//,
  /^\/[^/]+\/[^/]+\.git(\/|$)/i,
];

/** Whether hovering `link` may fetch its target ahead of the click. */
function isPreloadable(link) {
  if (!(link instanceof HTMLAnchorElement) || !link.hasAttribute("href")) return false;
  if (link.hasAttribute("download") || link.hasAttribute("preload")) return false;
  if (link.target && link.target !== "_self") return false;
  // Only boosted links: htmx swaps them in, so the preloaded response is actually used.
  if (link.closest("[hx-boost]")?.getAttribute("hx-boost") !== "true") return false;
  if (link.closest("[data-no-preload]")) return false;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin || !/^https?:$/.test(url.protocol)) return false;
  // In-page anchors and links to the page itself.
  if (url.pathname === location.pathname && url.search === location.search) return false;
  return !NO_PRELOAD.some((pattern) => pattern.test(url.pathname));
}

// htmx processes each boosted link individually; marking it here, before the preload
// extension's `afterProcessNode` hook runs for it, opts it in.
document.addEventListener("htmx:beforeProcessNode", (event) => {
  if (isPreloadable(event.target)) event.target.setAttribute("preload", "mouseover always");
});

document.addEventListener("htmx:configRequest", (event) => {
  event.detail.headers[EPOCH_HEADER] = epoch;
});

// ---------------------------------------------------------------- progress

const root = document.documentElement;
let pending = 0;

function settle() {
  pending = Math.max(0, pending - 1);
  if (pending === 0) root.classList.remove("nav-busy");
}

document.addEventListener("htmx:beforeSend", (event) => {
  if (!event.detail.requestConfig?.boosted) return;
  pending += 1;
  root.classList.add("nav-busy");
});

document.addEventListener("htmx:afterRequest", (event) => {
  const config = event.detail.requestConfig;
  if (config && config.verb !== "get") epoch = newEpoch();
  if (config?.boosted) settle();
});

// Back/forward to a page htmx has no snapshot of fetches it from the server.
document.addEventListener("htmx:historyCacheMiss", () => {
  pending += 1;
  root.classList.add("nav-busy");
});
document.addEventListener("htmx:historyRestore", (event) => {
  if (event.detail.cacheMiss) settle();
});
document.addEventListener("htmx:historyCacheMissLoadError", settle);

// ---------------------------------------------------------------- render time

/** Mirrors `timing::format`: `412 µs`, `3.1 ms`, `1.20 s`. */
function formatDuration(ms) {
  if (ms < 1) return `${Math.round(ms * 1000)} µs`;
  if (ms < 1000) return `${ms.toFixed(1)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

let renderTime = null;
const showRenderTime = () => {
  if (renderTime === null) return;
  for (const element of document.querySelectorAll("[data-render-time]")) element.textContent = renderTime;
};

document.addEventListener("htmx:afterRequest", (event) => {
  if (!event.detail.successful) return;
  const timing = event.detail.xhr?.getResponseHeader("Server-Timing");
  const match = timing && /(?:^|,)\s*render;dur=([\d.]+)/.exec(timing);
  if (!match) return;
  renderTime = formatDuration(Number(match[1]));
  showRenderTime();
});
// A full-page swap brings a new footer; it shows the same figure as the header.
document.addEventListener("htmx:afterSettle", showRenderTime);

// ---------------------------------------------------------------- partial swaps

/** The repository the current page belongs to (`/owner/repo`), if any. */
function currentScope() {
  return document.querySelector("[data-repo-scope]")?.getAttribute("data-repo-scope") ?? null;
}

document.addEventListener("htmx:beforeSwap", (event) => {
  const detail = event.detail;
  const config = detail.requestConfig;
  if (!config?.boosted || config.verb !== "get" || detail.isError || !detail.shouldSwap) return;
  const scope = currentScope();
  const main = document.getElementById("main");
  const response = detail.serverResponse;
  if (!scope || !main || typeof response !== "string") return;
  // A plain substring test: cheaper than parsing the page twice, and enough to know the
  // response is a page of the same repository with the same chrome.
  if (!response.includes(`data-repo-scope="${scope}"`) || !response.includes('id="main"')) return;

  detail.target = main;
  detail.selectOverride = "#main";
  const anchorLink = detail.elt?.getAttribute?.("href")?.includes("#");
  detail.swapOverride = anchorLink ? "outerHTML" : "outerHTML show:window:top";
  // The tabs (active tab, star count) come along out of band; read by htmx right
  // after this event, from the triggering element.
  detail.elt?.setAttribute?.("hx-select-oob", "#repo-subnav");
});

document.addEventListener("htmx:afterSwap", (event) => {
  event.detail.elt?.removeAttribute?.("hx-select-oob");
});

// ---------------------------------------------------------------- leaving large pages

// Leaving a large file or diff is costly in two htmx steps: the history snapshot clones
// and serialises the whole page, and the swap walks every outgoing element to clean it
// up (one event each; highlighted code has tens of thousands of spans). For such pages
// the snapshot is skipped (going back refetches, usually a 304) and the code and diff
// tables, which hold no htmx state, are dropped before htmx walks them. Both happen in
// the same task as the swap, so nothing is painted in between.
const LARGE_PAGE_ELEMENTS = 4000;
const HEAVY_CONTENT = ".code-lines, .diff-view";
const NO_SNAPSHOT_ID = "nav-no-snapshot";

const isLargePage = () => document.getElementsByTagName("*").length >= LARGE_PAGE_ELEMENTS;

function skipSnapshot() {
  if (document.getElementById(NO_SNAPSHOT_ID)) return;
  const marker = document.createElement("template");
  marker.id = NO_SNAPSHOT_ID;
  marker.setAttribute("hx-history", "false");
  document.body.append(marker);
}

function dropHeavyContent() {
  for (const element of document.querySelectorAll(HEAVY_CONTENT)) element.remove();
}

// Links and forms: the response is in hand and is about to be swapped in.
document.addEventListener("htmx:beforeSwap", (event) => {
  const detail = event.detail;
  if (!detail.requestConfig?.boosted || !detail.shouldSwap || !isLargePage()) return;
  skipSnapshot();
  dropHeavyContent();
});

// Back/forward: htmx snapshots the page first, then swaps in the cached or refetched
// one. This listener is registered before htmx's own, so it runs first; the content is
// only dropped once the replacement is in hand.
let leavingLargePage = false;
addEventListener("popstate", (event) => {
  leavingLargePage = Boolean(event.state?.htmx) && isLargePage();
  if (leavingLargePage) skipSnapshot();
});
for (const name of ["htmx:historyCacheHit", "htmx:historyCacheMissLoad"]) {
  document.addEventListener(name, () => {
    if (leavingLargePage) dropHeavyContent();
    leavingLargePage = false;
  });
}

const removeMarker = () => document.getElementById(NO_SNAPSHOT_ID)?.remove();
document.addEventListener("htmx:afterSwap", removeMarker);
document.addEventListener("htmx:historyRestore", removeMarker);

// ---------------------------------------------------------------- history cache

// htmx re-serialises its whole sessionStorage cache on every navigation, so one large
// snapshot slows every later navigation. Any that still get through are dropped (htmx
// stores an unreachable placeholder instead).
const MAX_SNAPSHOT_CHARS = 200_000;

document.addEventListener("htmx:historyItemCreated", (event) => {
  const item = event.detail.item;
  if (item.content.length > MAX_SNAPSHOT_CHARS) {
    item.content = "";
    item.url = "\u0000too-large";
  }
});

// ---------------------------------------------------------------- line links

// The file viewer's line numbers are plain cells (see repo/blob.html); a click links to
// the line like an `<a href="#L12">` would.
document.addEventListener("click", (event) => {
  const cell = event.target instanceof Element ? event.target.closest(".code-line .line-number[data-line]") : null;
  if (cell) location.hash = `L${cell.dataset.line}`;
});

// ---------------------------------------------------------------- focus

// After a boosted navigation, move focus to the new content so keyboard and screen
// reader users start there (the clicked link is usually gone). Links to an anchor keep
// htmx's scrolling to it.
document.addEventListener("htmx:afterSettle", (event) => {
  const config = event.detail.requestConfig;
  if (!config?.boosted || location.hash) return;
  const main = document.getElementById("main");
  if (!main || main.contains(document.activeElement)) return;
  main.setAttribute("tabindex", "-1");
  main.focus({ preventScroll: true });
});
