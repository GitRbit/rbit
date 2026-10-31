// Navigation speed and polish on top of htmx's boosted links (hx-boost on <body>).
// Every page works without this file; it only makes moving between pages faster.
//
// * Preloading lives in preload.js; it sends the same headers as the navigation that
//   follows (`navigationHeaders`), so the click is answered from the browser cache.
// * Navigation epoch: every htmx request carries a per-page-load value that changes
//   after any state-changing request. HTML varies on it, so a page preloaded before a
//   form post is never served after it.
// * Partial swaps: between pages of the same repository only <main> is replaced;
//   the repository tab bar is reconciled in place. Requests carry `Rbit-Partial` (the
//   repository scope), and the server then sends only the title, <main> and the tabs
//   (`partial.rs`); a full page is swapped the same way.
// * Progress bar after a short delay (CSS), and focus moved to <main> after navigating.
// * Render time: the footer's "rendered in" figure follows every htmx request, partial
//   swaps included, from the response's `Server-Timing` header (see `timing.rs`).

import { clearNavigationResponses, isCachedNavigationResponse, rememberNavigationResponse, reuseNavigationResponse } from "./repo-navigation-cache.js";
import { reconcileRepositoryBar } from "./repo-navigation-bar.js";
import { installUpdateNotice } from "./update-notice.js";

installUpdateNotice(document);

// Runtime feature changes require a fresh shell, assets and navigation history.
function retireLocalizedNavigation() {
  clearNavigationResponses();
  try { localStorage.removeItem("htmx-history-cache"); } catch { /* Storage can be unavailable. */ }
  globalThis.navigator?.serviceWorker?.controller?.postMessage({ type: "rbit-locale-changed" });
}

// A plain form submission creates a fresh document without an HTMX response event.
// Retire persisted history here as well, and notify other open tabs of the new identity.
const LOCALE_STORAGE = "rbit-locale-revision";
const initialLocale = document.body?.dataset?.localeRevision;
try {
  const previous = localStorage.getItem(LOCALE_STORAGE);
  if (previous && initialLocale && previous !== initialLocale) retireLocalizedNavigation();
  if (initialLocale) localStorage.setItem(LOCALE_STORAGE, initialLocale);
} catch { /* Preferences remain usable when storage is unavailable. */ }
globalThis.addEventListener?.("storage", (event) => {
  if (event.key !== LOCALE_STORAGE || !event.newValue || event.newValue === initialLocale) return;
  retireLocalizedNavigation();
  location.reload();
});
document.addEventListener("htmx:beforeOnLoad", (event) => {
  const changed = [["pluginRevision", "Rbit-Plugin-Revision"], ["localeRevision", "Rbit-Locale-Revision"]]
    .some(([key, header]) => {
      const current = document.body?.dataset?.[key];
      const next = event.detail.xhr?.getResponseHeader?.(header);
      return current && next && current !== next;
    });
  if (!changed) return;
  event.preventDefault();
  // Stop resources owned by an unpublished contribution before the full reload.
  document.dispatchEvent(new CustomEvent("rbit:plugin-revision", { detail: {
    revision: event.detail.xhr?.getResponseHeader?.("Rbit-Plugin-Revision"),
  } }));
  retireLocalizedNavigation();
  const config = event.detail.requestConfig;
  const navigation = config?.boosted && config.verb?.toLowerCase() === "get";
  // Deferred fragments and polls can have URLs which are not full HTML pages.
  location.assign(navigation ? (event.detail.xhr.responseURL || location.href) : location.href);
});


// Title-Case like any HTTP header name (the server lists them in `Vary` with that spelling).
const EPOCH_HEADER = "Rbit-Nav-Epoch";
const PARTIAL_HEADER = "Rbit-Partial";
const newEpoch = () => `${Date.now().toString(36)}.${Math.random().toString(36).slice(2, 8)}`;
let epoch = newEpoch();
let activeNavigation = null;

function abortNavigation() {
  const xhr = activeNavigation;
  activeNavigation = null;
  xhr?.abort();
}

/** The current navigation epoch; preload.js forgets its preloads when it changes. */
export const navigationEpoch = () => epoch;

/** The repository the current page belongs to (`/owner/repo`), if any. */
function currentScope() {
  return document.querySelector("[data-repo-scope]")?.getAttribute("data-repo-scope") ?? null;
}

/** The current page's repository scope if `url` is a page of the same repository. */
function partialScope(url) {
  const scope = currentScope();
  if (!scope) return null;
  const { pathname } = new URL(url, location.href);
  return pathname === scope || pathname.startsWith(`${scope}/`) ? scope : null;
}

/**
 * The headers a boosted GET of `url` sends from this page — every header the server's
 * `Vary` names. A preload sending exactly these shares the navigation's cache entry.
 */
export function navigationHeaders(url) {
  const headers = { "HX-Request": "true", [EPOCH_HEADER]: epoch };
  const revision = document.body?.dataset?.pluginRevision;
  if (revision) headers["Rbit-Plugin-Revision"] = revision;
  const locale = document.body?.dataset?.localeRevision;
  if (locale) headers["Rbit-Locale-Revision"] = locale;
  const scope = partialScope(url);
  if (scope) headers[PARTIAL_HEADER] = scope;
  return headers;
}

document.addEventListener("htmx:beforeRequest", (event) => {
  const config = event.detail.requestConfig;
  if (config?.verb && config.verb.toLowerCase() !== "get") {
    // Invalidate before the mutation starts, including requests which later fail.
    epoch = newEpoch();
    clearNavigationResponses();
    abortNavigation();
  } else if (config?.boosted && config.verb === "get") {
    // Different links have separate htmx request locks. The latest navigation
    // must win even when an earlier tab's server response arrives later.
    if (activeNavigation !== event.detail.xhr) abortNavigation();
    activeNavigation = event.detail.xhr;
    reuseNavigationResponse(event.detail, navigationHeaders(config.path));
  }
});

function rememberTabResponse(detail) {
  const config = detail.requestConfig;
  const tab = (config?.elt ?? detail.elt)?.closest?.('.repo-tabs a[data-repo-tab]');
  if (!tab || tab.dataset.repoTab === "settings" || !detail.successful || !config?.boosted || config.verb !== "get") return;
  const xhr = detail.xhr;
  if (isCachedNavigationResponse(xhr)) return;
  rememberNavigationResponse(config.path, { ...navigationHeaders(config.path), "Rbit-Nav-Epoch": config.headers?.[EPOCH_HEADER] ?? epoch }, {
    status: xhr.status, url: xhr.responseURL, text: xhr.responseText,
    // XHR deliberately does not expose Set-Cookie to scripts.
    getHeader: name => name.toLowerCase() === "set-cookie" ? null : xhr.getResponseHeader(name),
  });
}

// Seed the exact initial tab before Markdown, diagrams, and deferred content mutate
// its DOM. A deep file or filtered listing must never masquerade as the Code tab.
function seedCurrentTab() {
  const current = new URL(location.href);
  current.hash = "";
  const tab = [...document.querySelectorAll('.repo-tabs a[data-repo-tab]')].find(link => link.href === current.href && link.dataset.repoTab !== "settings");
  const main = document.getElementById("main");
  const bar = document.getElementById("repo-subnav");
  const title = document.querySelector("title");
  if (!tab || !main || !bar || !title) return;
  const copy = bar.cloneNode(true);
  copy.setAttribute("hx-swap-oob", "true");
  const headers = { "content-type": "text/html", "rbit-partial": "true", "cache-control": "private, max-age=30" };
  rememberNavigationResponse(current.href, navigationHeaders(current.href), {
    status: 200, url: current.href, text: `${title.outerHTML}\n${main.outerHTML}\n${copy.outerHTML}`,
    getHeader: name => headers[name.toLowerCase()] ?? null,
  });
}
seedCurrentTab();

document.addEventListener("htmx:configRequest", (event) => {
  const detail = event.detail;
  detail.headers[EPOCH_HEADER] = epoch;
  const revision = document.body?.dataset?.pluginRevision;
  if (revision) detail.headers["Rbit-Plugin-Revision"] = revision;
  const locale = document.body?.dataset?.localeRevision;
  if (locale) detail.headers["Rbit-Locale-Revision"] = locale;
  if (!detail.boosted || detail.verb !== "get") return;
  const scope = partialScope(detail.path);
  if (scope) detail.headers[PARTIAL_HEADER] = scope;
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
  if (activeNavigation === event.detail.xhr) activeNavigation = null;
  rememberTabResponse(event.detail);
  const config = event.detail.requestConfig;
  if (config && config.verb !== "get") epoch = newEpoch();
  if (config?.boosted) settle();
});

for (const name of ["pagehide", "popstate"]) addEventListener(name, () => {
  abortNavigation();
  clearNavigationResponses();
});
addEventListener("pageshow", event => {
  if (event.persisted) {
    epoch = newEpoch();
    clearNavigationResponses();
  }
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
/** Shows the last render time in `root` (by default the whole page). */
const showRenderTime = (root = document) => {
  if (renderTime === null) return;
  for (const element of root.querySelectorAll("[data-render-time]")) element.textContent = renderTime;
};

document.addEventListener("htmx:afterRequest", (event) => {
  if (!event.detail.successful) return;
  const timing = event.detail.xhr?.getResponseHeader("Server-Timing");
  const match = timing && /(?:^|,)\s*render;dur=([\d.]+)/.exec(timing);
  if (!match) return;
  renderTime = formatDuration(Number(match[1]));
  showRenderTime();
});
// A full-page swap brings a new footer; it shows the same figure as the header. Only
// the element swapped in can hold a new one.
document.addEventListener("htmx:afterSettle", (event) => {
  const root = event.detail.elt;
  if (root?.querySelectorAll) showRenderTime(root);
});

// ---------------------------------------------------------------- partial swaps

document.addEventListener("htmx:oobBeforeSwap", (event) => {
  const { target, fragment } = event.detail;
  if (target?.id !== "repo-subnav") return;
  const incoming = fragment?.querySelector('#repo-subnav');
  if (incoming && reconcileRepositoryBar(target, incoming)) event.detail.shouldSwap = false;
});

document.addEventListener("htmx:beforeSwap", (event) => {
  const detail = event.detail;
  const config = detail.requestConfig;
  if (!config?.boosted || config.verb !== "get" || detail.isError || !detail.shouldSwap) return;
  // The server sent only the title, <main> and the tabs (with hx-swap-oob).
  const partial = Boolean(detail.xhr?.getResponseHeader(PARTIAL_HEADER));
  const scope = currentScope();
  const main = document.getElementById("main");
  const response = detail.serverResponse;
  // A plain substring test: cheaper than parsing the page twice, and enough to know the
  // response is a page of the same repository with the same chrome.
  const sameRepository =
    scope && main && typeof response === "string" && response.includes(`data-repo-scope="${scope}"`) && response.includes('id="main"');
  if (!sameRepository) {
    // A partial page cannot replace the whole body (the page changed repository while the
    // request was in flight): load the page in full instead.
    if (partial) {
      detail.shouldSwap = false;
      location.assign(detail.xhr.responseURL || detail.pathInfo?.finalRequestPath);
    }
    return;
  }

  detail.target = main;
  const anchorLink = detail.elt?.getAttribute?.("href")?.includes("#");
  detail.swapOverride = anchorLink ? "outerHTML" : "outerHTML show:window:top";
  // Never an `hx-select` inherited from an ancestor of the link. A partial response has
  // <main> at its root, where the select finds it too; htmx swaps its out-of-band tabs
  // and reads its title before selecting.
  detail.selectOverride = "#main";
  if (partial) return;
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
// the snapshot is skipped (going back refetches, usually a 304) and the code and diffs
// (`.diff-view`, every row block and comment thread of a file), which hold no htmx state
// that outlives them, are dropped before htmx walks them. Both happen in the same task as
// the swap, so nothing is painted in between.
const LARGE_PAGE_ELEMENTS = 4000;
const HEAVY_CONTENT = ".code-lines, .diff-view";
const NO_SNAPSHOT_ID = "nav-no-snapshot";

// Asking the live collection for one late element walks the page only that far; its
// `length` would count every element, which on the pages this is for is the slow part.
const isLargePage = () => document.getElementsByTagName("*")[LARGE_PAGE_ELEMENTS - 1] !== undefined;

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

// htmx re-parses and re-serialises its whole sessionStorage cache on every navigation,
// so one large snapshot slows every later navigation. Any over this size are dropped
// (htmx stores an unreachable placeholder instead); going back to such a page refetches
// it, usually answered with a 304.
const MAX_SNAPSHOT_CHARS = 64_000;

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
  const tab = (config.elt ?? event.detail.elt)?.closest?.('.repo-tabs a[data-repo-tab]');
  // Stable tab links keep keyboard focus through a navigation.
  if (tab?.isConnected && tab.contains(document.activeElement)) return;
  const main = document.getElementById("main");
  if (!main || main.contains(document.activeElement)) return;
  main.setAttribute("tabindex", "-1");
  main.focus({ preventScroll: true });
});
