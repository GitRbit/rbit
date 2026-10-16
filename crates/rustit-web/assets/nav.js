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

const EPOCH_HEADER = "Rustit-Nav-Epoch";
const newEpoch = () => `${Date.now().toString(36)}.${Math.random().toString(36).slice(2, 8)}`;
let epoch = newEpoch();

// Mirrors `middleware::is_unsafe_to_preload` (the server's answer is authoritative; this
// only avoids sending requests that would be refused).
const NO_PRELOAD = [
  /^\/logout(\/|$)/,
  /^\/settings\/security\/two-factor/,
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
