// rbit service worker.
//
// * Immutable, content-hashed /assets/* — cache-first; every other asset uses HTTP caching.
// * HTML pages — network-first, falling back to the offline page when the network is down.
//   Navigation preload starts that request while the worker itself is still starting, so
//   a sleeping worker adds no latency to full page loads. Pages are never cached here:
//   they are per-user and the server decides their freshness.
// * Everything else (Git transport, form posts) — untouched.
//
// The server substitutes __VERSION__ with the asset build hash, so each deploy gets a fresh cache.

const VERSION = "__VERSION__";
const STATIC_CACHE = `rbit-static-${VERSION}`;
const OFFLINE_URL = "/offline";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(STATIC_CACHE).then((cache) => cache.add(new Request(OFFLINE_URL, { cache: "reload", credentials: "omit" }))),
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key.startsWith("rbit-static-") && key !== STATIC_CACHE).map((key) => caches.delete(key))))
      .then(() => self.registration.navigationPreload?.enable())
      .then(() => self.clients.claim()),
  );
});

// Refresh the anonymous offline document after a locale change. A failed refresh
// must preserve the existing fallback; content-hashed assets remain reusable.
self.addEventListener("message", (event) => {
  if (event.data?.type !== "rbit-locale-changed") return;
  event.waitUntil(caches.open(STATIC_CACHE).then((cache) => cache.add(new Request(OFFLINE_URL, { cache: "reload", credentials: "omit" }))).catch(() => {}));
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (url.pathname.startsWith("/assets/") && /^\?v=[a-f0-9]{12}$/.test(url.search)) {
    event.respondWith(
      (async () => {
        // Cache storage can be unavailable or full. It must never prevent a healthy
        // network response from reaching the page.
        const cache = await caches.open(STATIC_CACHE).catch(() => null);
        const cached = await cache?.match(request).catch(() => null);
        if (cached) return cached;
        const response = await fetch(request);
        const directives = (response.headers.get("Cache-Control") || "").toLowerCase().split(",").map((value) => value.trim());
        if (cache && response.ok && directives.includes("public") && directives.includes("immutable")
            && !directives.some((value) => /^(?:private|no-store|no-cache)(?:=|$)/.test(value))) {
          event.waitUntil(cache.put(request, response.clone()).catch(() => {}));
        }
        return response;
      })(),
    );
    return;
  }

  if (request.mode === "navigate") {
    event.respondWith(
      (async () => {
        try {
          return (await event.preloadResponse) || (await fetch(request));
        } catch {
          // Other applications (or a removed older worker) can have their own
          // /offline entry. Only this version's anonymous fallback is trusted.
          const cache = await caches.open(STATIC_CACHE).catch(() => null);
          return (await cache?.match(OFFLINE_URL).catch(() => null))
            || new Response("The server could not be reached.", { status: 503, headers: { "Content-Type": "text/plain" } });
        }
      })(),
    );
  }
});
