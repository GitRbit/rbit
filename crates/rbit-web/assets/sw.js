// rbit service worker.
//
// * /assets/*  — cache-first. URLs carry a content hash (?v=…), so entries never go stale.
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
    caches.open(STATIC_CACHE).then((cache) => cache.add(new Request(OFFLINE_URL, { cache: "reload" }))),
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== STATIC_CACHE).map((key) => caches.delete(key))))
      .then(() => self.registration.navigationPreload?.enable())
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(
      caches.open(STATIC_CACHE).then(async (cache) => {
        const cached = await cache.match(request);
        if (cached) return cached;
        const response = await fetch(request);
        if (response.ok) cache.put(request, response.clone());
        return response;
      }),
    );
    return;
  }

  if (request.mode === "navigate") {
    event.respondWith(
      (async () => {
        try {
          return (await event.preloadResponse) || (await fetch(request));
        } catch {
          return caches.match(OFFLINE_URL);
        }
      })(),
    );
  }
});
