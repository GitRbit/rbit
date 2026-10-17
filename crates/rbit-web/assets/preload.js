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
//   and the record is dropped when the navigation epoch changes. At most MAX_IN_FLIGHT
//   preloads run at a time.
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
  return !NO_PRELOAD.some((pattern) => pattern.test(url.pathname));
}

// ---------------------------------------------------------------- queue

/** When each URL (with its headers) was last preloaded, for the current epoch. */
const preloaded = new Map();
let preloadedEpoch = navigationEpoch();
const queue = [];
let inFlight = 0;

/** Queues a preload of `link`; false when it is not preloadable or already fresh. */
function preload(link, { soon = true } = {}) {
  if (!isPreloadable(link)) return false;
  if (preloadedEpoch !== navigationEpoch()) {
    preloaded.clear();
    preloadedEpoch = navigationEpoch();
  }
  const url = new URL(link.href, location.href);
  url.hash = "";
  const headers = navigationHeaders(url.href);
  const key = `${headers["Rbit-Partial"] ?? ""} ${url.href}`;
  const at = preloaded.get(key);
  if (at !== undefined && performance.now() - at < FRESH_MS) return false;
  preloaded.set(key, performance.now());

  const job = { key, url: url.href, headers, soon };
  // Intent goes first; viewport preloads wait their turn.
  if (soon) queue.unshift(job);
  else queue.push(job);
  while (queue.length > MAX_QUEUED) preloaded.delete(queue.pop().key);
  pump();
  return true;
}

function pump() {
  while (inFlight < MAX_IN_FLIGHT && queue.length > 0) {
    const job = queue.shift();
    inFlight += 1;
    fetch(job.url, {
      headers: { ...job.headers, "HX-Preloaded": "true" },
      credentials: "same-origin",
      priority: job.soon ? "high" : "low",
    })
      // Reading the body lets the browser finish storing it in the cache.
      .then((response) => response.arrayBuffer())
      .catch(() => preloaded.delete(job.key))
      .finally(() => {
        inFlight -= 1;
        pump();
      });
  }
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
  if (event.pointerType === "touch" || event.button !== 0) return;
  const link = linkOf(event.target);
  if (!link) return;
  cancelHover();
  preload(link);
});

// ---------------------------------------------------------------- touch

let touch = null;

function cancelTouch() {
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
    touch = { link, x, y, timer: setTimeout(() => (cancelTouch(), preload(link)), TOUCH_DELAY_MS) };
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
for (const type of ["touchend", "touchcancel"]) document.addEventListener(type, cancelTouch, { passive: true });
// Any scroll, of the page or of a scrolling box, means the finger was not tapping.
document.addEventListener("scroll", cancelTouch, { capture: true, passive: true });

// ---------------------------------------------------------------- visible links

const connection = navigator.connection;
const constrained = () => Boolean(connection?.saveData) || /(^|-)2g$/.test(connection?.effectiveType ?? "");
const whenIdle = (callback) =>
  "requestIdleCallback" in window ? requestIdleCallback(callback, { timeout: 2000 }) : setTimeout(callback, 200);

let budget = VISIBLE_BUDGET;
const observer =
  "IntersectionObserver" in window
    ? new IntersectionObserver((entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const link = entry.target;
          observer.unobserve(link);
          whenIdle(() => {
            if (budget > 0 && link.isConnected && !constrained() && preload(link, { soon: false })) budget -= 1;
          });
        }
      })
    : null;

/** Watches the page's `data-preload="visible"` links, with a fresh budget. */
function watchVisibleLinks() {
  if (!observer) return;
  observer.disconnect();
  budget = VISIBLE_BUDGET;
  if (constrained()) return;
  for (const link of document.querySelectorAll('a[data-preload="visible"]')) observer.observe(link);
}

watchVisibleLinks();
// After each navigation (partial swaps included) and back/forward.
document.addEventListener("htmx:afterSettle", (event) => {
  if (event.detail.requestConfig?.boosted) watchVisibleLinks();
});
document.addEventListener("htmx:historyRestore", watchVisibleLinks);
