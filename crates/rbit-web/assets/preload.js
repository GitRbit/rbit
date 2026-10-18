// Preloading for boosted links: a page starts loading when the viewer shows the intent to
// open it, so the click is answered from the browser cache. Every page works without it.
//
// * Intent: a mouse resting on a link for HOVER_DELAY_MS (moving off cancels it), a mouse
//   button pressed on it, or a finger resting on it for TOUCH_DELAY_MS without moving or
//   scrolling — so scrolling a list on a phone fetches nothing.
// * Ahead of intent: links marked `data-preload="visible"` (repository tabs, next-page
//   links) once they are on screen and the browser is idle, at most VISIBLE_BUDGET per
//   page, and never under Save-Data or on a 2g connection.
// * Each URL at most once per freshness window: the server keeps a preload fresh in the
//   browser cache for `PRELOAD_MAX_AGE_SECONDS` (rendered into <body data-preload-fresh>),
//   and records are bounded and dropped when the navigation epoch changes. At most
//   MAX_IN_FLIGHT preloads run at a time; obsolete work is aborted. Save-Data disables
//   all speculative requests, including hover and touch intent.
// * Cache key: the preload sends exactly the headers the navigation will send and the
//   server varies on (nav.js `navigationHeaders`: HX-Request, the epoch and, inside a
//   repository, Rbit-Partial); htmx's other headers (HX-Current-URL, HX-Target, …) are not
//   in `Vary`. It is a plain `fetch` — not `htmx.ajax`, which would run htmx's request
//   events (progress bar, epoch, swaps) for a response nobody swaps.
// * Safety: same-origin boosted links only, never the current page, downloads or the
//   paths in NO_PRELOAD, which mirrors the server's `middleware::is_unsafe_to_preload`
//   (the server's answer is authoritative). Preloads send `HX-Preloaded`.

import { navigationEpoch, navigationHeaders } from "./nav.js";

const HOVER_DELAY_MS = 80;
const TOUCH_DELAY_MS = 60;
// A finger moving further than this is scrolling, not tapping.
const TOUCH_SLOP_PX = 8;
const MAX_IN_FLIGHT = 2;
const MAX_QUEUED = 6;
const MAX_RECORDS = 128;
const VISIBLE_BUDGET = 4;
const FRESH_MS = (Number(document.body.dataset.preloadFresh) || 30) * 1000;

const NO_PRELOAD = [
  /^\/logout(\/|$)/,
  /^\/settings\/security/,
  // The second sign-in step, and (milestone 3, O) external sign-in under /login/{provider}.
  /^\/login\//,
  /^\/email\/verify\//,
  /^\/password\/reset\//,
  /^\/(api|assets)\//,
  /^\/sw\.js$/,
  /^\/(healthz|readyz|metrics)$/,
  /^\/[^/]+\/[^/]+\/(raw|archive|info|git-upload-pack|git-receive-pack)(\/|$)/,
  /^\/[^/]+\/[^/]+\/releases\/download\//,
  /^\/[^/]+\/[^/]+\.git(\/|$)/i,
];

// Opening a discussion marks the viewer's notifications read, so the server declines a
// signed-in viewer's preload of one (`routes::issues::show`, `routes::pulls::show`);
// anonymous viewers have no read state and preload them as usual.
const DISCUSSION = /^\/[^/]+\/[^/]+\/(issues|pull)\/\d+(\/|$)/;

const HTMX_REQUEST = ["get", "post", "put", "patch", "delete"].flatMap((verb) => [`[hx-${verb}]`, `[data-hx-${verb}]`]).join(",");

/**
 * Whether `link`'s target may be fetched ahead of the click. Checked only when the viewer
 * shows intent (event delegation on `document`), so pages with tens of thousands of links
 * cost nothing until then.
 */
function isPreloadable(link) {
  if (!(link instanceof HTMLAnchorElement) || !link.hasAttribute("href")) return false;
  if (link.hasAttribute("download") || (link.target && link.target !== "_self")) return false;
  // Only boosted links: htmx swaps them in, so the preloaded response is actually used.
  if (link.closest("[hx-boost]")?.getAttribute("hx-boost") !== "true") return false;
  // Links with their own htmx request (line-comment links on diffs) are not page navigations.
  if (link.matches(HTMX_REQUEST) || link.closest("[data-no-preload]")) return false;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin || !/^https?:$/.test(url.protocol)) return false;
  // In-page anchors and links to the page itself.
  if (url.pathname === location.pathname && url.search === location.search) return false;
  if ("signedIn" in document.body.dataset && DISCUSSION.test(url.pathname)) return false;
  return !NO_PRELOAD.some((pattern) => pattern.test(url.pathname));
}

// ---------------------------------------------------------------- queue

const connection = navigator.connection;
const constrained = () => Boolean(connection?.saveData) || /(^|-)2g$/.test(connection?.effectiveType ?? "");

/** Bounded pending and completed records for this epoch, keyed by navigation headers. */
const preloaded = new Map();
let preloadedEpoch = navigationEpoch();
const queue = [];
const active = new Set();
const navigations = new Set();
let suspended = false;
const navigating = () => suspended || navigations.size > 0;

function forget(job) {
  // An old, aborted request may settle after a new request for the same URL exists.
  if (preloaded.get(job.key) === job) preloaded.delete(job.key);
}

function cancel(job) {
  if (job.done) return;
  forget(job);
  const index = queue.indexOf(job);
  if (index !== -1) queue.splice(index, 1);
  job.controller?.abort();
}

function cancelObsolete(destination = null) {
  // A queued destination has not started yet: navigation will fetch it directly.
  for (const job of queue.splice(0)) forget(job);
  for (const job of active) {
    if (job.url !== destination) cancel(job);
  }
  cancelHover();
  finishTouch();
  stopVisibleLinks();
}

function synchronizeEpoch() {
  const epoch = navigationEpoch();
  if (preloadedEpoch === epoch) return;
  preloadedEpoch = epoch;
  cancelObsolete();
  preloaded.clear();
}

function pruneRecords() {
  const now = performance.now();
  for (const job of preloaded.values()) {
    if (job.done && now - job.at >= FRESH_MS) forget(job);
  }
  // Pending work is already bounded by the queue and active limits. Evict only
  // completed records, so an eviction cannot start duplicate in-flight requests.
  for (const job of preloaded.values()) {
    if (preloaded.size < MAX_RECORDS) break;
    if (job.done) forget(job);
  }
}

/** Returns newly queued work, or null when it would not help the navigation. */
function preload(link, { soon = true } = {}) {
  synchronizeEpoch();
  if (navigating() || connection?.saveData || !link.isConnected || !isPreloadable(link)) return null;
  const url = new URL(link.href, location.href);
  url.hash = "";
  const headers = navigationHeaders(url.href);
  const key = `${headers["Rbit-Partial"] ?? ""} ${url.href}`;
  const existing = preloaded.get(key);
  if (existing && (!existing.done || performance.now() - existing.at < FRESH_MS)) return null;
  pruneRecords();

  const job = { key, url: url.href, headers, soon, done: false, controller: null, at: 0 };
  preloaded.set(key, job);
  // Intent goes first; viewport preloads wait their turn.
  if (soon) queue.unshift(job);
  else queue.push(job);
  while (queue.length > MAX_QUEUED) forget(queue.pop());
  pump();
  return preloaded.get(key) === job ? job : null;
}

function pump() {
  synchronizeEpoch();
  if (navigating() || connection?.saveData) return;
  while (active.size < MAX_IN_FLIGHT && queue.length > 0) {
    const job = queue.shift();
    job.controller = new AbortController();
    job.at = performance.now();
    active.add(job);
    fetch(job.url, {
      headers: { ...job.headers, "HX-Preloaded": "true" },
      credentials: "same-origin",
      priority: job.soon ? "high" : "low",
      signal: job.controller.signal,
    })
      .then(drain)
      .then((successful) => {
        if (successful && !job.controller.signal.aborted) job.done = true;
        else forget(job);
      })
      .catch(() => forget(job))
      .finally(() => {
        // Abort is asynchronous. Keep its slot until the fetch/body actually settles.
        active.delete(job);
        pump();
      });
  }
}

async function drain(response) {
  if (!response.ok) {
    await response.body?.cancel();
    return false;
  }
  // Finish the browser's HTTP cache write without retaining the whole HTML body.
  const reader = response.body?.getReader();
  if (!reader) return true;
  try {
    while (!(await reader.read()).done) { /* Discard each chunk after consumption. */ }
  } finally {
    reader.releaseLock();
  }
  return true;
}

const linkOf = (target) => (target instanceof Element ? target.closest("a[href]") : null);

// ---------------------------------------------------------------- mouse and pen

let hover = null;

function cancelHover() {
  if (hover) clearTimeout(hover.timer);
  hover = null;
}

document.addEventListener("pointerover", (event) => {
  if (event.pointerType === "touch") return;
  const link = linkOf(event.target);
  if (link === hover?.link) return;
  cancelHover();
  if (!link || !isPreloadable(link)) return;
  hover = { link, timer: setTimeout(() => (cancelHover(), preload(link)), HOVER_DELAY_MS) };
});

document.addEventListener("pointerout", (event) => {
  if (!hover || (event.relatedTarget instanceof Node && hover.link.contains(event.relatedTarget))) return;
  if (event.target instanceof Node && hover.link.contains(event.target)) cancelHover();
});

document.addEventListener("pointerdown", (event) => {
  if (event.pointerType === "touch" || event.button !== 0 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
  const link = linkOf(event.target);
  if (!link) return;
  cancelHover();
  preload(link);
});

// ---------------------------------------------------------------- touch

let touch = null;

function cancelTouch() {
  if (!touch) return;
  if (touch.job) cancel(touch.job);
  finishTouch();
  pump();
}

function finishTouch() {
  if (touch) clearTimeout(touch.timer);
  touch = null;
}

document.addEventListener(
  "touchstart",
  (event) => {
    cancelTouch();
    const link = event.touches.length === 1 ? linkOf(event.target) : null;
    if (!link || !isPreloadable(link)) return;
    const { clientX: x, clientY: y } = event.touches[0];
    const intent = { link, x, y, job: null, timer: null };
    intent.timer = setTimeout(() => {
      if (touch === intent) intent.job = preload(link);
    }, TOUCH_DELAY_MS);
    touch = intent;
  },
  { passive: true },
);

document.addEventListener(
  "touchmove",
  (event) => {
    if (!touch) return;
    const point = event.touches[0];
    if (!point || Math.hypot(point.clientX - touch.x, point.clientY - touch.y) > TOUCH_SLOP_PX) cancelTouch();
  },
  { passive: true },
);

// A tap shorter than the delay goes straight to the click; htmx fetches the page then.
document.addEventListener("touchend", finishTouch, { passive: true });
document.addEventListener("touchcancel", cancelTouch, { passive: true });
// Any scroll, of the page or of a scrolling box, means the finger was not tapping.
document.addEventListener("scroll", cancelTouch, { capture: true, passive: true });

// ---------------------------------------------------------------- visible links

const whenIdle = (callback) =>
  "requestIdleCallback" in window ? requestIdleCallback(callback, { timeout: 2000 }) : setTimeout(callback, 200);
const cancelIdle = (handle) => "requestIdleCallback" in window ? cancelIdleCallback(handle) : clearTimeout(handle);

let budget = VISIBLE_BUDGET;
let visibleGeneration = 0;
const idle = new Set();
const observer =
  "IntersectionObserver" in window
    ? new IntersectionObserver((entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting || idle.size >= budget || navigating() || constrained()) continue;
          const link = entry.target;
          observer.unobserve(link);
          const generation = visibleGeneration;
          const handle = whenIdle(() => {
            idle.delete(handle);
            if (generation === visibleGeneration && budget > 0 && !constrained() && preload(link, { soon: false })) budget -= 1;
          });
          idle.add(handle);
        }
      })
    : null;

function stopVisibleLinks() {
  visibleGeneration += 1;
  observer?.disconnect();
  for (const handle of idle) cancelIdle(handle);
  idle.clear();
}

/** Watches the page's `data-preload="visible"` links, with a fresh budget. */
function watchVisibleLinks(resetBudget = true) {
  stopVisibleLinks();
  if (!observer) return;
  if (resetBudget) budget = VISIBLE_BUDGET;
  if (navigating() || constrained()) return;
  for (const link of document.querySelectorAll('a[data-preload="visible"]')) observer.observe(link);
}

watchVisibleLinks();
document.addEventListener("htmx:beforeSend", (event) => {
  if (!event.detail.requestConfig?.boosted) return;
  const xhr = event.detail.xhr;
  navigations.add(xhr);
  // An overlapping swap can detach the link that originated this request. htmx's
  // failure events then never reach document, but the XHR still reports termination.
  for (const type of ["error", "abort", "timeout"]) {
    xhr.addEventListener(type, () => finishNavigation(xhr), { once: true });
  }
  const config = event.detail.requestConfig;
  const path = config.verb === "get" ? config.path : null;
  const destination = path ? new URL(path, location.href) : null;
  if (destination) destination.hash = "";
  cancelObsolete(destination?.href);
});
// nav.js is imported above, so its epoch rotation runs before this listener.
document.addEventListener("htmx:afterRequest", (event) => {
  synchronizeEpoch();
  if (event.detail.requestConfig?.boosted && (!event.detail.successful || event.detail.xhr?.status === 204)) {
    finishNavigation(event.detail.xhr);
  }
});

function resume() {
  if (navigating()) return;
  synchronizeEpoch();
  watchVisibleLinks();
  pump();
}

function finishNavigation(xhr) {
  // htmx can navigate through different links concurrently. An earlier response
  // must not restart speculation while another navigation is still pending.
  if (navigations.delete(xhr)) resume();
}

// After each navigation (partial swaps included) and back/forward.
document.addEventListener("htmx:afterSettle", (event) => {
  if (event.detail.requestConfig?.boosted) finishNavigation(event.detail.xhr);
});
function resumeHistory() {
  suspended = false;
  resume();
}
document.addEventListener("htmx:historyRestore", resumeHistory);
document.addEventListener("htmx:historyCacheMissLoadError", resumeHistory);
addEventListener("popstate", (event) => {
  if (!event.state?.htmx) return;
  suspended = true;
  cancelObsolete();
});
addEventListener("pagehide", () => {
  suspended = true;
  navigations.clear();
  cancelObsolete();
});
addEventListener("pageshow", (event) => {
  if (event.persisted) resumeHistory();
});
connection?.addEventListener?.("change", () => {
  if (connection.saveData) cancelObsolete();
  else if (constrained()) {
    for (const job of [...queue, ...active]) if (!job.soon) cancel(job);
  }
  watchVisibleLinks(false);
  pump();
});
