// A small response cache for the repository's primary links. htmx still owns the
// request, swap and history: only an individual, already-open XHR's transport is
// replaced when its exact response is available (or already being prefetched).
const TABS = new Set(['code', 'issues', 'pulls', 'commits', 'branches', 'tags', 'releases']);
const MAX_RESPONSE_BYTES = 1024 * 1024;
const MAX_TOTAL_BYTES = 4 * MAX_RESPONSE_BYTES;
const MAX_RESPONSES = 8;
const FRESH_MS = 30000;
const PRELOAD_WAIT_MS = 250;
const responses = new Map();
const pending = new Map();
const cachedTransports = new WeakSet();
let generation = 0;
let totalBytes = 0;

const RESPONSE_HEADERS = ['Content-Type', 'Cache-Control', 'Rbit-Partial', 'Server-Timing', 'Rbit-Update-Available', 'Rbit-Plugin-Revision', 'Rbit-Locale-Revision', 'Content-Language', 'ETag', 'Last-Modified', 'Vary'];
const EFFECT_HEADERS = ['Set-Cookie', 'HX-Location', 'HX-Redirect', 'HX-Refresh', 'HX-Retarget', 'HX-Reswap', 'HX-Reselect', 'HX-Push-Url', 'HX-Replace-Url', 'HX-Trigger', 'HX-Trigger-After-Swap', 'HX-Trigger-After-Settle'];

/** Synthetic deliveries must not refresh the original response's freshness time. */
export const isCachedNavigationResponse = (xhr) => cachedTransports.has(xhr);

function readHeader(headers, name) {
  if (typeof headers?.get === 'function') return headers.get(name);
  const key = Object.keys(headers ?? {}).find((key) => key.toLowerCase() === name.toLowerCase());
  return key ? headers[key] : null;
}

function requestKey(url, headers) {
  try {
    const target = new URL(url, location.href);
    const scope = readHeader(headers, 'Rbit-Partial');
    const epoch = readHeader(headers, 'Rbit-Nav-Epoch');
    if (target.origin !== location.origin || !/^https?:$/.test(target.protocol) || !scope || !epoch) return null;
    // Restrict caching to list pages. Discussion details mark notifications read;
    // settings and other routes may contain secrets or stateful GETs.
    const suffix = target.pathname.slice(scope.length);
    if (!target.pathname.startsWith(scope) || !/^(?:\/?|\/(?:issues|pulls|branches|tags|releases)|\/commits(?:\/[^?]*)?)$/.test(suffix)) return null;
    target.hash = '';
    return { key: JSON.stringify([target.href, scope, epoch, readHeader(headers, 'Rbit-Plugin-Revision'), readHeader(headers, 'Rbit-Locale-Revision')]), url: target.href, scope };
  } catch { return null; }
}

function forget(key) {
  const entry = responses.get(key);
  if (entry) totalBytes -= entry.size;
  responses.delete(key);
}

function fresh(key) {
  const entry = responses.get(key);
  if (!entry) return null;
  if (performance.now() - entry.at >= FRESH_MS) {
    forget(key);
    return null;
  }
  responses.delete(key);
  responses.set(key, entry);
  return entry;
}

function validate(request, response) {
  if (response.status !== 200 || response.redirected) return false;
  try {
    const target = new URL(response.url || request.url, location.href);
    target.hash = '';
    if (target.href !== request.url) return false;
  } catch { return false; }
  const get = response.getHeader;
  if (!/^text\/html(?:\s*;|$)/i.test(get('Content-Type') ?? '')) return false;
  if (/(?:^|,)\s*no-store\b/i.test(get('Cache-Control') ?? '')) return false;
  if (EFFECT_HEADERS.some((name) => get(name) !== null && get(name) !== undefined)) return false;
  return true;
}

function remember(request, response, expectedGeneration) {
  if (generation !== expectedGeneration || !validate(request, response)) return false;
  const text = response.text;
  if (typeof text !== 'string' || text.length > MAX_RESPONSE_BYTES) return false;
  const size = new TextEncoder().encode(text).byteLength;
  if (size > MAX_RESPONSE_BYTES) return false;
  // Require the same repository's actual subnav and main, even if a proxy or
  // permission change returned a successful HTML login/error page.
  const scope = request.scope.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  if (!new RegExp(`<div\\s+id="repo-subnav"[^>]*\\sdata-repo-scope="${scope}"(?:\\s|>)`).test(text) || !/<main\b[^>]*\bid="main"(?:\s|>)/.test(text)) return false;
  const headers = new Map(RESPONSE_HEADERS.map((name) => [name.toLowerCase(), response.getHeader(name)]).filter(([, value]) => value !== null && value !== undefined));
  forget(request.key);
  responses.set(request.key, { text, size, headers, url: request.url, at: performance.now() });
  totalBytes += size;
  while (responses.size > MAX_RESPONSES || totalBytes > MAX_TOTAL_BYTES) forget(responses.keys().next().value);
  return true;
}

/** Remember pristine HTML from a successful real boosted navigation or initial SSR. */
export function rememberNavigationResponse(url, headers, response) {
  const request = requestKey(url, headers);
  return Boolean(request && remember(request, response, generation));
}

/** Avoid speculative network work for an already fresh SSR or visited response. */
export function hasNavigationResponse(url, headers) {
  const request = requestKey(url, headers);
  return Boolean(request && fresh(request.key));
}

/** Invalidates old responses and wakes waiters without aborting another module's fetch. */
export function clearNavigationResponses() {
  generation++;
  responses.clear();
  totalBytes = 0;
  for (const load of pending.values()) load.invalidate();
  pending.clear();
}

async function readBounded(response) {
  const length = Number(response.headers.get('Content-Length'));
  if (length > MAX_RESPONSE_BYTES) {
    await response.body?.cancel();
    return null;
  }
  if (!response.body?.getReader) return null;
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = '';
  let bytes = 0;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      bytes += chunk.value.byteLength;
      if (bytes > MAX_RESPONSE_BYTES) {
        await reader.cancel();
        return null;
      }
      text += decoder.decode(chunk.value, { stream: true });
    }
    return text + decoder.decode();
  } finally { reader.releaseLock(); }
}

/** Owns body consumption of this fetch; resolves true only for reusable HTML. */
export function trackNavigationPreload(url, headers, responsePromise, signal) {
  const request = requestKey(url, headers);
  if (!request) return Promise.resolve(false);
  const existing = pending.get(request.key);
  if (existing) {
    // The caller normally deduplicates before fetching. If two callers race,
    // share the first load and release the redundant response body.
    Promise.resolve(responsePromise).then((response) => response.body?.cancel()).catch(() => {});
    return existing.promise.then(Boolean);
  }
  while (pending.size >= MAX_RESPONSES) {
    const oldest = pending.keys().next().value;
    pending.get(oldest).invalidate();
    pending.delete(oldest);
  }
  const started = generation;
  let invalidate;
  let active = true;
  const invalidated = new Promise((resolve) => { invalidate = () => { active = false; resolve(null); }; });
  const load = { invalidate, promise: null };
  const work = (async () => {
    try {
      const fetched = await responsePromise;
      const response = { status: fetched.status, url: fetched.url, redirected: fetched.redirected, getHeader: (name) => fetched.headers.get(name) };
      if (!active || signal?.aborted || started !== generation || !validate(request, response)) {
        await fetched.body?.cancel();
        return null;
      }
      const text = await readBounded(fetched);
      if (text === null || !active || signal?.aborted || started !== generation) return null;
      return remember(request, { ...response, text }, started) ? fresh(request.key) : null;
    } catch { return null; }
  })();
  load.promise = Promise.race([work, invalidated]);
  pending.set(request.key, load);
  signal?.addEventListener('abort', invalidate, { once: true });
  return load.promise.then(Boolean).finally(() => {
    signal?.removeEventListener('abort', invalidate);
    if (pending.get(request.key) === load) pending.delete(request.key);
  });
}

/**
 * Called at htmx:beforeRequest. An ordinary opened XHR remains untouched on misses.
 * Cached delivery dispatches real EventTarget events, so htmx's normal onload and
 * any request observers run in their usual order after htmx:beforeSend.
 */
export function reuseNavigationResponse(detail, headers) {
  const config = detail.requestConfig;
  const link = detail.elt ?? config?.elt;
  if (!config?.boosted || config.verb !== 'get' || !TABS.has(link?.dataset?.repoTab) || !link.closest?.('#repo-subnav .repo-tabs')) return false;
  const request = requestKey(detail.pathInfo?.finalRequestPath ?? config.path, headers);
  if (!request) return false;
  const cached = fresh(request.key);
  const load = pending.get(request.key);
  if (!cached && !load) return false;
  const xhr = detail.xhr;
  if (!xhr || xhr.readyState !== 1) return false;
  const started = generation;
  const names = ['send', 'abort', 'responseText', 'response', 'responseURL', 'status', 'statusText', 'readyState', 'getResponseHeader', 'getAllResponseHeaders'];
  const previous = new Map(names.map((name) => [name, Object.getOwnPropertyDescriptor(xhr, name)]));
  const nativeSend = xhr.send;
  const nativeAbort = xhr.abort;
  let sent = false;
  let settled = false;
  let native = false;
  let entry = null;
  let state = 1;
  let timer;
  let preloadTimer;
  const restore = () => {
    for (const name of names) {
      const descriptor = previous.get(name);
      if (descriptor) Object.defineProperty(xhr, name, descriptor);
      else delete xhr[name];
    }
  };
  const emit = (name) => xhr.dispatchEvent(new Event(name));
  const finish = (name) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    clearTimeout(preloadTimer);
    state = name === 'abort' ? 0 : 4;
    emit('readystatechange');
    emit(name);
    emit('loadend');
  };
  try {
    const properties = {
      responseText: { get: () => entry?.text ?? '' }, response: { get: () => entry?.text ?? '' },
      responseURL: { get: () => entry?.url ?? '' }, status: { get: () => entry ? 200 : 0 },
      statusText: { get: () => entry ? 'OK' : '' }, readyState: { get: () => state },
      getResponseHeader: { value: (name) => entry?.headers.get(name.toLowerCase()) ?? null },
      getAllResponseHeaders: { value: () => entry ? [...entry.headers].map(([name, value]) => `${name}: ${value}\r\n`).join('') : '' },
      abort: { value: () => {
        if (native) return nativeAbort.call(xhr);
        if (!settled) {
          // A synthetic XHR has never sent bytes; do not abort its shared preload.
          finish('abort');
        }
      } },
      send: { value: (body) => {
        if (sent || settled) return;
        sent = true;
        emit('loadstart');
        if (xhr.timeout > 0) timer = setTimeout(() => finish('timeout'), xhr.timeout);
        // A click must never wait indefinitely for speculation that has stalled.
        const result = cached ? Promise.resolve(cached) : Promise.race([
          load.promise,
          new Promise((resolve) => { preloadTimer = setTimeout(() => resolve(null), PRELOAD_WAIT_MS); }),
        ]);
        result.then((candidate) => {
          if (settled) return;
          clearTimeout(preloadTimer);
          if (!candidate || started !== generation || performance.now() - candidate.at >= FRESH_MS) {
            clearTimeout(timer);
            native = true;
            restore();
            nativeSend.call(xhr, body);
            return;
          }
          entry = candidate;
          cachedTransports.add(xhr);
          finish('load');
        });
      } },
    };
    for (const [name, descriptor] of Object.entries(properties)) Object.defineProperty(xhr, name, { configurable: true, ...descriptor });
    return true;
  } catch {
    // Some browser or extension could make an instance property non-configurable.
    // Decline enhancement before htmx sends anything rather than break navigation.
    restore();
    return false;
  }
}
