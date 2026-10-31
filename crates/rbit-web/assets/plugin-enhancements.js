// Only reviewed, host-shipped modules can enhance runtime contributions. DOM
// attributes are hints and must match the active registry in the host shell.
const DEFINITIONS = Object.freeze([
  Object.freeze({ id: "actions.log", owner: "rbit.actions", rootId: "job", path: "/assets/actions-log.js", matchesPage: path => path.length <= 2048 && /^\/[^/]+\/[^/]+\/actions\/runs\/[0-9]{1,20}\/jobs\/[0-9]{1,20}$/.test(path) }),
]);
const MAX_RESOURCES = 256;
const noop = () => {};

function generation(value) {
  return typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value) && BigInt(value) <= 18446744073709551615n;
}

/** Resources belong to one logical root, page and provider generation. */
export function createEnhancementScope({ document, root, current = () => true, timers = globalThis, identity = {}, onError = noop }) {
  const controller = new AbortController();
  const resources = new Set();
  const report = error => { try { onError(error); } catch { /* Diagnostics cannot prevent cleanup. */ } };
  const run = callback => { try { callback(); } catch (error) { report(error); } };
  const live = () => !controller.signal.aborted && current() && Boolean(root());
  function track(callback) {
    let active = true;
    const release = () => {
      if (!active) return;
      active = false;
      resources.delete(release);
      run(callback);
    };
    if (controller.signal.aborted) release();
    else if (resources.size >= MAX_RESOURCES) {
      release();
      throw new RangeError("Enhancement resource limit exceeded");
    } else resources.add(release);
    return release;
  }
  const context = Object.freeze({
    document,
    ...identity,
    signal: controller.signal,
    root: () => live() ? root() : null,
    onDispose: track,
    listen(target, type, callback, options = {}) {
      if (!live()) return noop;
      const capture = typeof options === "boolean" ? options : Boolean(options.capture);
      const listener = event => { if (live()) run(() => callback(event)); };
      target.addEventListener(type, listener, { ...(typeof options === "object" ? options : { capture }), signal: controller.signal });
      return track(() => target.removeEventListener(type, listener, capture));
    },
    timeout(callback, delay = 0) {
      if (!live()) return noop;
      let release = noop;
      const handle = timers.setTimeout(() => { release(); if (live()) run(callback); }, Math.max(0, Math.min(Number(delay) || 0, 86400000)));
      release = track(() => timers.clearTimeout(handle));
      return release;
    },
    interval(callback, delay = 16) {
      if (!live()) return noop;
      const handle = timers.setInterval(() => { if (live()) run(callback); }, Math.max(16, Math.min(Number(delay) || 16, 86400000)));
      return track(() => timers.clearInterval(handle));
    },
    observe(create, target, callback, options) {
      if (!live()) return noop;
      const observer = create((...args) => { if (live()) run(() => callback(...args)); });
      const release = track(() => observer.disconnect());
      try { observer.observe(target, options); } catch (error) { release(); throw error; }
      return release;
    },
  });
  return {
    context,
    dispose() {
      if (controller.signal.aborted) return;
      controller.abort();
      for (const release of [...resources].reverse()) release();
    },
  };
}

/** No code is loaded until the shell registry and live root agree. */
export function createEnhancementHost({
  document, window = globalThis.window, location = window.location,
  load = url => import(url), onError = noop,
}) {
  const mounted = new Map();
  const suspended = new WeakSet();
  let registrySource;
  let registry = new Map();
  let stopped = false;
  let blocked = false;
  let started = false;
  let hidden = false;
  let restoring = false;
  let queued = false;
  const listeners = [];
  let observer = null;
  const page = () => location.pathname + location.search;
  let observedPage = page();
  const report = error => { try { onError(error); } catch { /* Optional enhancement. */ } };
  function approved() {
    const source = document.body?.dataset?.pluginEnhancements ?? "";
    if (source === registrySource) return registry;
    registrySource = source;
    registry = new Map();
    if (source.length > 16384) return registry;
    try {
      const rows = JSON.parse(source);
      if (!Array.isArray(rows) || rows.length > 32) return registry;
      for (const row of rows) {
        const definition = DEFINITIONS.find(item => item.id === row?.id && item.owner === row?.owner);
        if (!definition || !generation(row.generation) || registry.has(row.id)) {
          registry = new Map();
          return registry;
        }
        registry.set(row.id, row);
      }
    } catch { /* A malformed registry never authorizes execution. */ }
    return registry;
  }
  function candidate(definition) {
    if (!definition.matchesPage(location.pathname)) return null;
    const owner = approved().get(definition.id);
    if (!owner) return null;
    const roots = document.querySelectorAll("#" + definition.rootId);
    if (roots.length !== 1) return null;
    const root = roots[0];
    if (!root.isConnected || suspended.has(root)
      || root.getAttribute("data-plugin-owner") !== owner.owner
      || root.getAttribute("data-plugin-generation") !== owner.generation
      || root.getAttribute("data-plugin-enhancement") !== definition.id) return null;
    try {
      const url = new URL(root.getAttribute("data-plugin-module"), location.href);
      if (url.origin !== location.origin || url.pathname !== definition.path
        || !/^\?v=[a-f0-9]{12}$/.test(url.search) || url.hash || url.username || url.password) return null;
      const key = JSON.stringify([definition.id, owner.owner, owner.generation, definition.rootId, page(), url.href]);
      return { root, url: url.href, key, owner: owner.owner, generation: owner.generation };
    } catch { return null; }
  }
  function retire(entry) {
    if (mounted.get(entry.definition.id) === entry) mounted.delete(entry.definition.id);
    entry.scope.dispose();
    if (!mounted.size && observer) { observer.disconnect(); observer = null; }
  }
  function retireAll() { for (const entry of [...mounted.values()]) retire(entry); }
  function current(entry) {
    return !stopped && !blocked && !hidden && !restoring && mounted.get(entry.definition.id) === entry
      && candidate(entry.definition)?.key === entry.key;
  }
  async function activate(entry) {
    try {
      const module = await load(entry.url);
      if (!current(entry)) { retire(entry); return; }
      if (typeof module.mount !== "function") throw new TypeError("Enhancement has no mount");
      // Even an asynchronous mount owns an already-cancellable resource scope.
      const dispose = await module.mount(entry.scope.context.root(), entry.scope.context);
      if (typeof dispose === "function") entry.scope.context.onDispose(dispose);
      if (!current(entry)) retire(entry);
    } catch (error) {
      retire(entry);
      report(error);
    }
  }
  function reconcile() {
    if (stopped || blocked || hidden || restoring) return;
    observedPage = page();
    for (const definition of DEFINITIONS) {
      const desired = candidate(definition);
      let entry = mounted.get(definition.id);
      if (entry && entry.key !== desired?.key) { retire(entry); entry = null; }
      if (!desired || entry) continue;
      entry = { url: desired.url, key: desired.key, definition };
      entry.scope = createEnhancementScope({
        document,
        identity: { id: definition.id, owner: desired.owner, generation: desired.generation, rootId: definition.rootId, page: page() },
        current: () => current(entry),
        root: () => { const next = candidate(definition); return next?.key === entry.key ? next.root : null; },
        onError: report,
      });
      mounted.set(definition.id, entry);
      watchRoot();
      void activate(entry);
    }
  }
  function watchRoot() {
    if (observer || typeof window.MutationObserver !== "function") return;
    try {
      observer = new window.MutationObserver(queueReconcile);
      observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: [
        "data-plugin-owner", "data-plugin-generation", "data-plugin-enhancement", "data-plugin-module", "data-plugin-enhancements",
      ] });
    } catch (error) { observer?.disconnect(); observer = null; report(error); }
  }
  function queueReconcile() {
    if (queued || stopped) return;
    queued = true;
    queueMicrotask(() => { queued = false; reconcile(); });
  }
  function listen(target, type, callback) {
    target.addEventListener(type, callback);
    listeners.push(() => target.removeEventListener(type, callback));
  }
  return {
    reconcile,
    start() {
      if (started || stopped) return;
      started = true;
      listen(document, "htmx:beforeSwap", event => {
        if (event.detail?.shouldSwap !== false && event.detail?.requestConfig?.boosted) { restoring = false; retireAll(); }
      });
      listen(document, "htmx:beforeCleanupElement", event => {
        const element = event.detail?.elt ?? event.target;
        for (const entry of mounted.values()) {
          const root = candidate(entry.definition)?.root;
          if (root && (element === root || element?.contains?.(root))) suspended.add(root);
        }
        // HTMX replaces polling roots synchronously. An identical replacement keeps
        // logical state; removal without replacement disposes at this boundary.
        queueReconcile();
      });
      listen(document, "htmx:afterSwap", reconcile);
      listen(document, "htmx:afterSettle", reconcile);
      listen(document, "htmx:historyRestore", () => { retireAll(); restoring = false; reconcile(); });
      listen(document, "rbit:plugin-revision", () => { blocked = true; retireAll(); });
      listen(window, "popstate", () => {
        const next = page();
        if (next === observedPage) return;
        observedPage = next;
        restoring = true; retireAll();
      });
      listen(window, "pagehide", () => { hidden = true; retireAll(); });
      listen(window, "pageshow", () => { hidden = false; restoring = false; reconcile(); });

      reconcile();
    },
    dispose() {
      if (stopped) return;
      stopped = true;
      retireAll();
      for (const remove of listeners.splice(0)) remove();
    },
  };
}
