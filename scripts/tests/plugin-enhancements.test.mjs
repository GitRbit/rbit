import assert from "node:assert/strict";
import test from "node:test";
import { createEnhancementHost, createEnhancementScope } from "../../crates/rbit-web/assets/plugin-enhancements.js";

class Events {
  listeners = new Map();
  addEventListener(type, callback) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(callback); this.listeners.set(type, listeners);
  }
  removeEventListener(type, callback) { this.listeners.get(type)?.delete(callback); }
  fire(type, event = {}) {
    for (const callback of [...(this.listeners.get(type) ?? [])]) {
      if (this.listeners.get(type)?.has(callback)) callback(event);
    }
  }
  count() { return [...this.listeners.values()].reduce((count, listeners) => count + listeners.size, 0); }
}
const pending = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const flush = async () => { for (let index = 0; index < 12; index++) await Promise.resolve(); };
const moduleUrl = "/assets/actions-log.js?v=0123456789ab";

function fixture(load, generation = "9007199254740993") {
  const document = new Events();
  const window = new Events();
  const location = new URL("https://git.example/alice/rocket/actions/runs/1/jobs/2");
  const observers = [];
  window.MutationObserver = class {
    constructor(callback) { this.callback = callback; this.disconnected = false; observers.push(this); }
    observe() {}
    disconnect() { this.disconnected = true; }
  };
  document.body = { dataset: {} }; document.documentElement = {};
  let roots = [];
  document.querySelectorAll = selector => selector === "#job" ? roots : [];
  function registry(value = generation) {
    document.body.dataset.pluginEnhancements = JSON.stringify([{ id: "actions.log", owner: "rbit.actions", generation: value }]);
  }
  function root(value = generation, url = moduleUrl) {
    const attributes = new Map(Object.entries({
      "data-plugin-owner": "rbit.actions", "data-plugin-generation": value,
      "data-plugin-enhancement": "actions.log", "data-plugin-module": url,
    }));
    return { attributes, ownerDocument: document, isConnected: true, getAttribute: key => attributes.get(key) ?? null, contains(other) { return this === other; } };
  }
  registry(); roots = [root()];
  const errors = [];
  const host = createEnhancementHost({ document, window, location, load, onError: error => errors.push(error) });
  return {
    document, window, location, observers, errors, host, root, registry,
    get current() { return roots[0]; },
    replace(value) { for (const root of roots) root.isConnected = false; roots = value ? [value] : []; },
    duplicate() { roots.push(root()); },
  };
}

test("DOM hints require a valid host registry, exact owner/generation, and allowlisted hashed module", async () => {
  for (const url of [
    "https://evil.example/assets/actions-log.js?v=0123456789ab",
    "/assets/app.js?v=0123456789ab", "javascript:alert(1)", moduleUrl + "#x",
    "/assets/actions-log.js", moduleUrl + "&extra=1", "https://user:pass@git.example" + moduleUrl,
  ]) {
    let loads = 0;
    const app = fixture(() => { loads++; return Promise.resolve({ mount() {} }); });
    app.replace(app.root(undefined, url)); app.host.start(); await flush();
    assert.equal(loads, 0, url); app.host.dispose();
  }
  for (const mutation of [
    app => { delete app.document.body.dataset.pluginEnhancements; },
    app => app.registry("18446744073709551616"),
    app => app.registry("01"),
    app => app.registry(12),
    app => app.registry("2"),
    app => app.current.attributes.set("data-plugin-owner", "foreign.plugin"),
    app => app.duplicate(),
    app => { app.location.pathname = "/alice/rocket"; },
    app => { app.location.pathname = "/alice/rocket/issues/1"; },
    app => { app.location.pathname = "/alice/rocket/actions/runs/not-an-id/jobs/2"; },
    app => { app.document.body.dataset.pluginEnhancements = "x".repeat(16385); },
  ]) {
    let loads = 0;
    const app = fixture(() => { loads++; return Promise.resolve({ mount() {} }); });
    mutation(app); app.host.start(); await flush(); assert.equal(loads, 0); app.host.dispose();
  }
});

test("late imports cannot mount a removed root or a previous generation", async () => {
  const imports = [];
  const mounts = [];
  const app = fixture(() => { const request = pending(); imports.push(request); return request.promise; });
  app.host.start();
  app.document.fire("htmx:beforeCleanupElement", { detail: { elt: app.current } });
  app.replace(app.root("9007199254740994")); app.registry("9007199254740994");
  app.document.fire("htmx:afterSwap");
  assert.equal(imports.length, 2);
  imports[0].resolve({ mount: () => { throw new Error("stale module mounted"); } });
  imports[1].resolve({ mount: (root, context) => { mounts.push({ root, context }); } });
  await flush();
  assert.equal(mounts.length, 1);
  assert.equal(mounts[0].context.generation, "9007199254740994");
  assert.equal(mounts[0].root, app.current);
  assert.equal(app.errors.length, 0);
  app.host.dispose();
});

test("same-page polling binds the replacement root without losing logical module state", async () => {
  let mounts = 0, context, seen;
  const app = fixture(async () => ({ mount(_root, scope) {
    mounts++; context = scope;
    scope.listen(app.document, "inspect", () => { seen = scope.root(); });
  } }));
  app.host.start(); await flush();
  const original = app.current;
  app.document.fire("htmx:beforeCleanupElement", { detail: { elt: original } });
  assert.equal(context.root(), null, "the root is suspended before DOM cleanup");
  app.replace(app.root()); app.document.fire("htmx:afterSwap"); await flush();
  app.document.fire("inspect");
  assert.equal(mounts, 1); assert.equal(seen, app.current);
  assert.notEqual(seen, original); assert.equal(context.signal.aborted, false);
  app.location.hash = "#step-2"; app.host.reconcile(); await flush();
  assert.equal(mounts, 1, "fragment changes do not replace the page scope");
  app.location.search = "?view=full"; app.host.reconcile(); await flush();
  assert.equal(mounts, 2, "page query identity changes retire the previous mount");
  app.host.dispose();
});

test("an asynchronous mount returns its disposer after cancellation without leaking resources", async () => {
  const mounting = pending(); let context, early = 0, late = 0, calls = 0;
  const app = fixture(async () => ({ mount(_root, scope) {
    calls++; context = scope; scope.onDispose(() => { early++; }); return mounting.promise;
  } }));
  app.host.start(); await flush();
  app.document.fire("rbit:plugin-revision");
  assert.equal(context.signal.aborted, true); assert.equal(early, 1);
  mounting.resolve(() => { late++; }); await flush();
  assert.equal(late, 1);
  app.document.fire("htmx:historyRestore"); app.window.fire("pageshow"); app.host.reconcile(); await flush();
  assert.equal(calls, 1, "a revision-invalidated shell cannot remount its stale metadata");
  app.host.dispose(); assert.equal(late, 1);
});

test("cleanup removes listeners, cancels timers and observers, and isolates callback/disposer errors", () => {
  const document = new Events(), tasks = new Map(), errors = []; let next = 0, callbacks = 0, disconnected = 0, observerCallback;
  const timers = {
    setTimeout(callback) { const id = ++next; tasks.set(id, callback); return id; },
    clearTimeout(id) { tasks.delete(id); },
    setInterval(callback) { const id = ++next; tasks.set(id, callback); return id; },
    clearInterval(id) { tasks.delete(id); },
  };
  const scope = createEnhancementScope({ document, root: () => ({}), timers, onError: error => errors.push(error) });
  scope.context.listen(document, "event", () => { callbacks++; throw new Error("listener"); });
  scope.context.timeout(() => { callbacks++; }, 0); scope.context.interval(() => { callbacks++; }, 0);
  scope.context.observe(callback => { observerCallback = callback; return { observe() {}, disconnect() { disconnected++; } }; }, {}, () => { callbacks++; });
  let cleaned = 0;
  scope.context.onDispose(() => { cleaned++; }); scope.context.onDispose(() => { throw new Error("cleanup"); });
  document.fire("event"); assert.equal(callbacks, 1); assert.equal(errors.length, 1);
  const late = [...tasks.values()];
  scope.dispose(); scope.dispose();
  assert.equal(document.count(), 0); assert.equal(tasks.size, 0); assert.equal(disconnected, 1); assert.equal(cleaned, 1);
  for (const callback of late) callback(); observerCallback(); document.fire("event");
  assert.equal(callbacks, 1); assert.equal(errors.length, 2);
  scope.context.onDispose(() => { cleaned++; }); assert.equal(cleaned, 2, "late cleanup runs immediately");
});

test("resource limits also clean the rejected allocation and completed timeouts release their slot", () => {
  let cleaned = 0; const tasks = new Map(); let id = 0;
  const scope = createEnhancementScope({ document: new Events(), root: () => ({}), timers: {
    setTimeout(callback) { const key = ++id; tasks.set(key, callback); return key; }, clearTimeout(key) { tasks.delete(key); },
  } });
  for (let index = 0; index < 300; index++) {
    scope.context.timeout(() => {}, 0);
    [...tasks.values()][0]();
  }
  assert.equal(tasks.size, 0);
  for (let index = 0; index < 256; index++) scope.context.onDispose(() => { cleaned++; });
  assert.throws(() => scope.context.onDispose(() => { cleaned++; }), RangeError);
  assert.equal(cleaned, 1); scope.dispose(); assert.equal(cleaned, 257);
});

test("navigation, history, pagehide and native root removal have deterministic disposal", async () => {
  const contexts = [];
  const app = fixture(async () => ({ mount(_root, context) { contexts.push(context); } }));
  app.host.start(); await flush();
  app.document.fire("htmx:beforeSwap", { detail: { shouldSwap: false, requestConfig: { boosted: true } } });
  assert.equal(contexts[0].signal.aborted, false);
  app.document.fire("htmx:beforeSwap", { detail: { shouldSwap: true, requestConfig: { boosted: true } } });
  assert.equal(contexts[0].signal.aborted, true);
  app.document.fire("htmx:afterSettle"); await flush();
  app.location.hash = "#step-2"; app.window.fire("popstate");
  assert.equal(contexts[1].signal.aborted, false, "hash-only back/forward does not await a nonexistent HTMX restore");
  app.location.pathname = "/alice/rocket/actions/runs/1/jobs/3";
  app.window.fire("popstate"); assert.equal(contexts[1].signal.aborted, true);
  app.document.fire("htmx:afterSwap"); await flush(); assert.equal(contexts.length, 2);
  app.document.fire("htmx:historyRestore"); await flush(); assert.equal(contexts.length, 3);
  app.window.fire("pagehide"); assert.equal(contexts[2].signal.aborted, true);
  app.document.fire("htmx:afterSettle"); await flush(); assert.equal(contexts.length, 3);
  app.window.fire("pageshow"); await flush(); assert.equal(contexts.length, 4);
  app.replace(null); app.observers[0].callback(); await flush();
  assert.equal(contexts[3].signal.aborted, true);
  app.host.dispose(); assert.equal(app.document.count(), 0); assert.equal(app.window.count(), 0);
  assert.equal(app.observers[0].disconnected, true);
});

test("failed imports and failed mounts retain the SSR fallback and can retry on a later refresh", async () => {
  let attempts = 0, disposed = 0;
  const app = fixture(async () => {
    attempts++;
    if (attempts === 1) throw new Error("network");
    return { mount(_root, context) { context.onDispose(() => { disposed++; }); if (attempts === 2) throw new Error("mount"); } };
  });
  app.host.start(); await flush(); assert.equal(app.errors.length, 1);
  app.host.reconcile(); await flush(); assert.equal(disposed, 1);
  app.host.reconcile(); await flush(); assert.equal(attempts, 3);
  app.host.dispose(); assert.equal(disposed, 2);
});
