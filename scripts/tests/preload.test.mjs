import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const assets = new URL("../../crates/rbit-web/assets/", import.meta.url);
const source = (await readFile(new URL("preload.js", assets), "utf8"))
  .replace(/^import .*;\r?\n/gm, "");
const navigation = (await readFile(new URL("nav.js", assets), "utf8")).replace(/^import .*;\r?\n/gm, "").replace(/^export /gm, "");
const navigationCache = (await readFile(new URL("repo-navigation-cache.js", assets), "utf8")).replace(/^export /gm, "");
const updateNotice = (await readFile(new URL("update-notice.js", assets), "utf8")).replace(/^export /gm, "");
const navigationBar = (await readFile(new URL("repo-navigation-bar.js", assets), "utf8")).replace(/^export /gm, "");
const origin = "https://git.example.com";

function page({ saveData = false, effectiveType = "4g", idleCallback = true, signedIn = false, visiblePaths = [] } = {}) {
  const listeners = new Map();
  const timers = new Map();
  const idle = new Map();
  const requests = [];
  const visible = [];
  const observed = new Set();
  let clock = 0;
  let nextHandle = 0;
  let intersection;
  let currentNavigation;
  const location = new URL(`${origin}/alice/repo`);
  const document = {
    body: { dataset: { preloadFresh: "30", ...(signedIn ? { signedIn: "" } : {}) } },
    documentElement: { classList: { add() {}, remove() {} } },
    querySelector(selector) {
      return selector === "[data-repo-scope]" ? { getAttribute: () => "/alice/repo" } : null;
    },
    querySelectorAll(selector) { return selector === 'a[data-preload="visible"]' ? visible : []; },
    getElementById() { return null; },
    getElementsByTagName() { return []; },
    addEventListener(name, callback) {
      const registered = listeners.get(name) ?? [];
      registered.push(callback);
      listeners.set(name, registered);
    },
  };
  class Element {
    contains(other) { return this === other; }
  }
  class Anchor extends Element {
    constructor(path, attributes = {}) {
      super();
      this.href = new URL(path, location.href).href;
      this.attributes = { href: path, ...attributes };
      this.target = attributes.target;
      this.isConnected = true;
    }
    hasAttribute(name) { return Object.hasOwn(this.attributes, name); }
    getAttribute(name) { return this.attributes[name] ?? null; }
    closest(selector) {
      if (selector === "a[href]") return this;
      if (selector === ".repo-tabs") return this.hasAttribute("data-repo-tab") ? {} : null;
      if (selector === "[hx-boost]") return { getAttribute: () => this.attributes["hx-boost"] ?? "true" };
      if (selector === "[data-no-preload]") return this.hasAttribute("data-no-preload") ? this : null;
      return null;
    }
    matches() { return Object.keys(this.attributes).some((name) => /^(data-)?hx-(get|post|put|patch|delete)$/.test(name)); }
  }
  visible.push(...visiblePaths.map((path) => new Anchor(path, { "data-preload": "visible" })));
  const connection = {
    saveData, effectiveType,
    addEventListener(name, callback) { document.addEventListener(`connection:${name}`, callback); },
  };
  const context = vm.createContext({
    URL, AbortController, Element, Node: Element, HTMLAnchorElement: Anchor, TextEncoder, TextDecoder, Event,
    document, location, navigator: { connection }, performance: { now: () => clock },
    addEventListener: document.addEventListener,
    setTimeout(callback, delay) {
      const handle = ++nextHandle;
      timers.set(handle, { callback, at: clock + delay });
      return handle;
    },
    clearTimeout(handle) { timers.delete(handle); },
    IntersectionObserver: class {
      constructor(callback) { intersection = callback; }
      observe(target) { observed.add(target); }
      unobserve(target) { observed.delete(target); }
      disconnect() { observed.clear(); }
    },
    fetch(url, options) {
      let resolve;
      let reject;
      const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
      const request = {
        url, options,
        reads: 0, released: false, bodyCancelled: false,
        headers({ ok = true, chunks = 0 } = {}) {
          let finish;
          let fail;
          const body = new Promise((yes, no) => { finish = yes; fail = no; });
          this.finishBody = () => finish({ done: true });
          this.failBody = () => fail(new Error("Body aborted"));
          resolve({
            ok,
            body: {
              cancel() { request.bodyCancelled = true; finish({ done: true }); },
              getReader() {
                return {
                  read() {
                    request.reads += 1;
                    return request.reads <= chunks ? Promise.resolve({ done: false, value: new Uint8Array(1024) }) : body;
                  },
                  releaseLock() { request.released = true; },
                };
              },
            },
            arrayBuffer() { throw new Error("Preloads must not buffer whole HTML responses"); },
          });
        },
        complete(options) { this.headers(options); this.finishBody(); },
        fail() { reject(new Error("Network failed or request aborted")); },
      };
      // Deliberately do not settle on abort. Real abort completion is asynchronous;
      // tests choose the ordering to expose slot accounting and stale callbacks.
      requests.push(request);
      return promise;
    },
  });
  context.window = context;
  if (idleCallback) {
    context.requestIdleCallback = (callback) => {
      const handle = ++nextHandle;
      idle.set(handle, callback);
      return handle;
    };
    context.cancelIdleCallback = (handle) => idle.delete(handle);
  }
  Object.assign(context, vm.runInContext(`(() => { ${navigationCache}\nreturn { clearNavigationResponses, hasNavigationResponse, isCachedNavigationResponse, rememberNavigationResponse, reuseNavigationResponse, trackNavigationPreload }; })()`, context));
  Object.assign(context, vm.runInContext(`(() => { ${navigationBar}\nreturn { reconcileRepositoryBar }; })()`, context));
  Object.assign(context, vm.runInContext(`(() => { ${updateNotice}\nreturn { installUpdateNotice }; })()`, context));
  // Run the actual nav module first, just as preload.js's import does in the browser.
  // Module locals stay private while the two public functions are shared.
  Object.assign(context, vm.runInContext(`(() => { ${navigation}\nreturn { navigationEpoch, navigationHeaders }; })()`, context));
  vm.runInContext(source, context);
  const dispatch = (name, event = {}) => {
    for (const callback of listeners.get(name) ?? []) callback({ detail: {}, ...event });
  };
  return {
    requests, connection, idle, document, visible, observed,
    navigationHeaders: path => context.navigationHeaders(path),
    link: (path, attributes) => new Anchor(path, attributes),
    dispatch,
    press(link) { dispatch("pointerdown", { target: link, pointerType: "mouse", button: 0 }); },
    hover(link) { dispatch("pointerover", { target: link, pointerType: "mouse" }); },
    touch(link) { dispatch("touchstart", { target: link, touches: [{ clientX: 0, clientY: 0 }] }); },
    navigate(path) {
      currentNavigation = new EventTarget();
      currentNavigation.getResponseHeader = () => null;
      dispatch("htmx:beforeSend", { detail: { requestConfig: { boosted: true, verb: "get", path }, xhr: currentNavigation } });
      return currentNavigation;
    },
    settle(xhr = currentNavigation) { dispatch("htmx:afterSettle", { detail: { requestConfig: { boosted: true, verb: "get" }, xhr } }); },
    mutate() { dispatch("htmx:afterRequest", { detail: { requestConfig: { verb: "post" }, successful: true } }); },
    intersect(...links) { intersection(links.map((target) => ({ target, isIntersecting: true }))); },
    runIdle() {
      for (const [handle, callback] of [...idle]) {
        idle.delete(handle);
        callback();
      }
    },
    advance(milliseconds) {
      clock += milliseconds;
      for (const [handle, timer] of [...timers]) {
        if (timer.at > clock) continue;
        timers.delete(handle);
        timer.callback();
      }
    },
  };
}

// Flush fetch, body-consumption and finally continuations without wall-clock sleeps.
async function flush() {
  for (let i = 0; i < 12; i += 1) await Promise.resolve();
}

test("intent stays quiet until hover/touch delay and shares pending and fresh work", async () => {
  const app = page();
  const link = app.link("/alice/repo/tree/main");
  app.hover(link);
  app.advance(79);
  assert.equal(app.requests.length, 0);
  app.dispatch("pointerout", { target: link, relatedTarget: null });
  app.advance(1);
  assert.equal(app.requests.length, 0, "moving away before intent sends no request");
  app.touch(link);
  app.advance(59);
  app.dispatch("touchend");
  app.advance(1);
  assert.equal(app.requests.length, 0, "a short tap goes directly to navigation");
  app.press(link);
  app.press(link);
  assert.equal(app.requests.length, 1);
  app.requests[0].complete();
  await flush();
  app.press(link);
  assert.equal(app.requests.length, 1, "fresh work is not requested again");
  app.advance(30_001);
  app.press(link);
  assert.equal(app.requests.length, 2, "expired work may be refreshed");
});

test("latest repository navigation aborts the previous request and history cancels pending swaps", () => {
  const app = page();
  const first = { aborted: 0, abort() { this.aborted++; } };
  const second = { aborted: 0, abort() { this.aborted++; } };
  const start = xhr => app.dispatch("htmx:beforeRequest", { detail: { xhr, requestConfig: { boosted: true, verb: "get", path: "/alice/repo/issues" } } });
  start(first); start(second);
  assert.equal(first.aborted, 1);
  app.dispatch("htmx:afterRequest", { detail: { xhr: first, requestConfig: { boosted: true, verb: "get" } } });
  app.dispatch("popstate", { state: { htmx: true } });
  assert.equal(second.aborted, 1, "a late completion cannot release the newer navigation");
});

test("mutations invalidate tab snapshots and abort navigation before the form is sent", () => {
  const app = page();
  const before = app.navigationHeaders("/alice/repo/issues")["Rbit-Nav-Epoch"];
  const xhr = { aborted: false, abort() { this.aborted = true; } };
  app.dispatch("htmx:beforeRequest", { detail: { xhr, requestConfig: { boosted: true, verb: "get", path: "/alice/repo/issues" } } });
  app.dispatch("htmx:beforeRequest", { detail: { requestConfig: { verb: "post" } } });
  assert.equal(xhr.aborted, true);
  assert.notEqual(app.navigationHeaders("/alice/repo/issues")["Rbit-Nav-Epoch"], before);
});

test("repository tabs preload immediately on pointer intent and keyboard focus, respecting Save-Data", () => {
  for (const intent of ["pointerover", "focusin"]) {
    for (const saveData of [false, true]) {
      const app = page({ saveData });
      const link = app.link("/alice/repo/issues", { "data-repo-tab": "issues" });
      app.dispatch(intent, { target: link, pointerType: "mouse" });
      app.advance(0);
      assert.equal(app.requests.length, saveData ? 0 : 1);
    }
  }
});

test("a stable repository tab retains keyboard focus when htmx settles the new main", () => {
  const app = page();
  const tab = { isConnected: true, closest() { return this; }, contains(node) { return node === this; } };
  let focusStolen = false;
  const main = { closest() { return null; }, contains() { return false; }, setAttribute() {}, focus() { focusStolen = true; } };
  app.document.activeElement = tab;
  app.document.getElementById = id => id === "main" ? main : null;
  app.dispatch("htmx:afterSettle", { detail: { elt: main, requestConfig: { boosted: true, verb: "get", elt: tab } } });
  assert.equal(focusStolen, false);
});

test("navigation cancels stale work and prevents six queued requests from reaching the network", async () => {
  const app = page();
  for (let i = 0; i < 8; i += 1) app.press(app.link(`/alice/repo/tree/${i}`));
  assert.equal(app.requests.length, 2);
  app.navigate("/alice/repo/tree/0");
  assert.equal(app.requests[0].options.signal.aborted, false, "an active destination can still warm its cache entry");
  assert.equal(app.requests[1].options.signal.aborted, true);
  app.requests[0].complete();
  app.requests[1].fail();
  await flush();
  app.press(app.link("/alice/repo/tree/another"));
  assert.equal(app.requests.length, 2, "navigation stops both queued work and new speculation");
  app.settle();
  await flush();
  assert.equal(app.requests.length, 2, "stale queued requests stay cancelled after navigation");
  app.press(app.link("/alice/repo/tree/new"));
  assert.equal(app.requests.length, 3);
});

test("epoch cancellation retains active limits and old failures cannot delete new jobs", async () => {
  const app = page();
  const link = app.link("/alice/repo/tree/main");
  app.press(link);
  app.press(app.link("/alice/repo/commits/main"));
  app.press(app.link("/alice/repo/tags"));
  const oldEpoch = app.requests[0].options.headers["Rbit-Nav-Epoch"];
  app.mutate();
  assert.ok(app.requests.every((request) => request.options.signal.aborted));
  app.press(link);
  assert.equal(app.requests.length, 2, "aborting does not free a slot before fetch settles");
  app.requests[1].fail();
  await flush();
  assert.equal(app.requests.length, 3);
  assert.notEqual(app.requests[2].options.headers["Rbit-Nav-Epoch"], oldEpoch);
  app.requests[0].fail();
  await flush();
  app.press(link);
  assert.equal(app.requests.length, 3, "old rejection cannot erase the replacement's pending record");
  app.requests[2].complete();
  await flush();
  app.press(link);
  assert.equal(app.requests.length, 3);
});

test("overlapping navigations resume speculation only after every request finishes", () => {
  for (const firstCompletion of ["settle", "failed", "empty"]) {
    const app = page();
    const first = app.navigate("/alice/repo/tree/first");
    const second = app.navigate("/alice/repo/tree/second");
    if (firstCompletion === "settle") app.settle(first);
    else {
      first.status = firstCompletion === "empty" ? 204 : 500;
      app.dispatch("htmx:afterRequest", {
        detail: { requestConfig: { boosted: true, verb: "get" }, successful: firstCompletion === "empty", xhr: first },
      });
    }
    const link = app.link("/alice/repo/tree/intent");
    app.press(link);
    app.intersect(link);
    app.runIdle();
    assert.equal(app.requests.length, 0, `${firstCompletion} for an earlier request cannot resume speculation`);
    app.settle(second);
    app.press(link);
    assert.equal(app.requests.length, 1);
  }
});

test("detached navigation origins cannot leave speculation suspended after XHR failure", () => {
  for (const terminal of ["error", "abort", "timeout"]) {
    const app = page();
    const first = app.navigate("/alice/repo/tree/first");
    const detached = app.navigate("/alice/repo/tree/detached");
    app.settle(first);
    const link = app.link("/alice/repo/tree/intent");
    app.press(link);
    assert.equal(app.requests.length, 0, "the other navigation still owns its pending slot");
    // The first swap removed the second request's origin. No htmx:afterRequest
    // bubbles to document; only the native XHR lifecycle event remains observable.
    detached.dispatchEvent(new Event(terminal));
    app.press(link);
    assert.equal(app.requests.length, 1, `${terminal} cleans up even with a detached origin`);
    // Duplicate terminal notifications are harmless and do not remove another XHR.
    const next = app.navigate("/alice/repo/tree/next");
    detached.dispatchEvent(new Event(terminal));
    app.press(app.link("/alice/repo/tree/later"));
    assert.equal(app.requests.length, 1);
    app.settle(next);
  }
});

test("touch scrolling cancels work both before and after the delay; a completed tap keeps it", async () => {
  const app = page();
  const link = app.link("/alice/repo/tree/main");
  app.touch(link);
  app.advance(30);
  app.dispatch("scroll");
  app.advance(60);
  assert.equal(app.requests.length, 0);
  app.touch(link);
  app.advance(60);
  assert.equal(app.requests.length, 1);
  app.dispatch("touchmove", { touches: [{ clientX: 0, clientY: 9 }] });
  assert.equal(app.requests[0].options.signal.aborted, true);
  app.requests[0].fail();
  await flush();
  app.touch(link);
  app.advance(60);
  app.dispatch("touchend");
  app.dispatch("scroll");
  assert.equal(app.requests[1].options.signal.aborted, false, "scroll after a completed tap is unrelated");
});

test("Save-Data suppresses every trigger and changing connection cancels existing speculation", async () => {
  const app = page({ saveData: true });
  const link = app.link("/alice/repo/tree/main");
  app.hover(link);
  app.advance(80);
  app.press(link);
  app.touch(link);
  app.advance(60);
  app.intersect(link);
  app.runIdle();
  assert.equal(app.requests.length, 0);
  app.connection.saveData = false;
  app.dispatch("connection:change");
  app.press(link);
  assert.equal(app.requests.length, 1);
  app.connection.saveData = true;
  app.dispatch("connection:change");
  assert.equal(app.requests[0].options.signal.aborted, true);
  app.requests[0].fail();
  await flush();
  assert.equal(app.requests.length, 1);
});

test("response streams keep their slots until body completion or cancellation and release readers", async () => {
  const app = page();
  app.press(app.link("/alice/repo/tree/first"));
  app.press(app.link("/alice/repo/tree/second"));
  for (const request of app.requests) request.headers({ chunks: 2 });
  await flush();
  assert.ok(app.requests.every((request) => request.reads === 3 && !request.released));
  app.navigate("/alice/repo/tree/destination");
  assert.ok(app.requests.every((request) => request.options.signal.aborted));
  app.settle();
  app.press(app.link("/alice/repo/tree/new"));
  assert.equal(app.requests.length, 2, "headers do not release a slot while the body is pending");
  app.requests[0].failBody();
  await flush();
  assert.equal(app.requests.length, 3);
  assert.equal(app.requests[0].released, true);
  app.requests[1].finishBody();
  await flush();
  assert.equal(app.requests[1].released, true);
  app.requests[2].complete({ chunks: 2 });
  await flush();
  assert.equal(app.requests[2].released, true);
});

test("failed responses retry and failed or empty navigation resumes speculation", async () => {
  const app = page();
  const link = app.link("/alice/repo/tree/main");
  app.press(link);
  app.requests[0].complete({ ok: false });
  await flush();
  assert.equal(app.requests[0].bodyCancelled, true, "error response bodies are not downloaded");
  app.press(link);
  assert.equal(app.requests.length, 2, "a server error must not count as fresh work");
  app.requests[1].complete();
  await flush();
  for (const [successful, status] of [[false, 500], [true, 204]]) {
    const xhr = app.navigate("/alice/repo/failed");
    xhr.status = status;
    app.dispatch("htmx:afterRequest", {
      detail: { requestConfig: { boosted: true, verb: "get" }, successful, xhr },
    });
    app.press(app.link(`/alice/repo/tree/${status}`));
  }
  assert.equal(app.requests.length, 4, "responses without a page swap must not disable future preloads");
});

test("history and bfcache transitions cancel stale work while unrelated popstate leaves intent enabled", async () => {
  const app = page();
  app.dispatch("popstate", { state: null });
  app.press(app.link("/alice/repo/tree/first"));
  assert.equal(app.requests.length, 1);
  app.dispatch("popstate", { state: { htmx: true } });
  assert.equal(app.requests[0].options.signal.aborted, true);
  app.requests[0].fail();
  await flush();
  app.dispatch("htmx:historyRestore", { detail: { cacheMiss: false } });
  app.press(app.link("/alice/repo/tree/second"));
  assert.equal(app.requests.length, 2);
  app.dispatch("pagehide");
  assert.equal(app.requests[1].options.signal.aborted, true);
  app.requests[1].fail();
  await flush();
  app.dispatch("pageshow", { persisted: true });
  app.press(app.link("/alice/repo/tree/third"));
  assert.equal(app.requests.length, 3);
});

test("visible scheduling is bounded and obsolete idle callbacks cannot fetch after navigation", async () => {
  for (const idleCallback of [true, false]) {
    const app = page({ idleCallback });
    const links = Array.from({ length: 100 }, (_, i) => app.link(`/alice/repo/tree/${i}`));
    app.intersect(...links);
    if (idleCallback) assert.equal(app.idle.size, 7, "visible idle callbacks are bounded too");
    const staleCallbacks = [...app.idle.values()];
    app.navigate("/alice/repo/tree/destination");
    app.settle();
    for (const callback of staleCallbacks) callback();
    app.advance(200);
    app.runIdle();
    assert.equal(app.requests.length, 0, "a callback scheduled by the outgoing page is obsolete");
    app.intersect(...links);
    app.runIdle();
    app.advance(200);
    assert.equal(app.requests.length, 2);
    for (const request of app.requests) request.complete();
    await flush();
    assert.equal(app.requests.length, 4);
    for (let index = 2; index < 7; index++) {
      app.requests[index].complete();
      await flush();
    }
    assert.equal(app.requests.length, 7, "visible speculation stops after the per-page budget");
  }
});

test("retained records stay bounded without evicting pending work", async () => {
  const app = page();
  for (let i = 0; i < 140; i += 1) {
    app.press(app.link(`/alice/repo/tree/${i}`));
    app.requests.at(-1).complete();
    await flush();
  }
  app.press(app.link("/alice/repo/tree/139"));
  assert.equal(app.requests.length, 140, "a recent record remains fresh");
  app.press(app.link("/alice/repo/tree/0"));
  assert.equal(app.requests.length, 141, "the oldest record was evicted within the freshness window");
  for (let i = 140; i < 160; i += 1) app.press(app.link(`/alice/repo/tree/${i}`));
  app.press(app.link("/alice/repo/tree/0"));
  assert.equal(app.requests.length, 142, "pending records are not evicted or duplicated under queue pressure");
});

test("unsafe links stay excluded and eligible fetches preserve navigation cache and auth headers", () => {
  const app = page({ signedIn: true });
  for (const path of [
    "/logout", "/settings/security", "/login/github", "/email/verify/secret", "/password/reset/secret",
    "/api/repos", "/assets/app.js", "/sw.js", "/healthz", "/readyz", "/metrics",
    "/alice/repo/raw/main/file", "/alice/repo/archive/main.zip", "/alice/repo/info/refs",
    "/alice/repo/git-upload-pack", "/alice/repo/git-receive-pack", "/alice/repo/releases/download/tag/asset",
    "/alice/repo/issues/12", "/alice/repo/pull/12", "/alice/repo.git", "/alice/repo#anchor",
    "https://another.example.com/alice/repo",
  ]) app.press(app.link(path));
  for (const attributes of [{ download: "" }, { target: "_blank" }, { "hx-get": "/fragment" }, { "hx-boost": "false" }, { "data-no-preload": "" }]) {
    app.press(app.link("/alice/repo/tree/main", attributes));
  }
  for (const modifier of ["ctrlKey", "metaKey", "altKey", "shiftKey"]) {
    app.dispatch("pointerdown", { target: app.link("/alice/repo/tree/main"), pointerType: "mouse", button: 0, [modifier]: true });
  }
  assert.equal(app.requests.length, 0);
  app.press(app.link("/alice/repo/tree/main#L20"));
  const request = app.requests[0];
  assert.equal(request.url, `${origin}/alice/repo/tree/main`);
  assert.equal(request.options.credentials, "same-origin");
  assert.equal(request.options.headers["HX-Request"], "true");
  assert.equal(request.options.headers["HX-Preloaded"], "true");
  assert.equal(request.options.headers["Rbit-Partial"], "/alice/repo");
  assert.ok(request.options.headers["Rbit-Nav-Epoch"]);
  assert.equal(request.options.cache, undefined, "HTTP caching remains server-authoritative");
  app.press(app.link("/other/repository/tree/main"));
  assert.equal(app.requests[1].options.headers["Rbit-Partial"], undefined);
});

test("discussions are preloaded only for anonymous viewers, who have no read state", () => {
  const anonymous = page();
  anonymous.press(anonymous.link("/alice/repo/issues/12"));
  anonymous.press(anonymous.link("/alice/repo/pull/12/files"));
  assert.equal(anonymous.requests.length, 2);
  const signedIn = page({ signedIn: true });
  signedIn.press(signedIn.link("/alice/repo/issues/12"));
  signedIn.press(signedIn.link("/alice/repo/pull/12/files"));
  signedIn.press(signedIn.link("/alice/repo/issues"));
  assert.deepEqual(signedIn.requests.map((request) => new URL(request.url).pathname), ["/alice/repo/issues"]);
});

test("render time reaches a swapped-in footer without scanning the page again", () => {
  const app = page();
  const pageScans = [];
  const scan = app.document.querySelectorAll;
  app.document.querySelectorAll = (selector) => { pageScans.push(selector); return scan(selector); };
  const xhr = { getResponseHeader: (name) => name === "Server-Timing" ? "render;dur=3.14" : null };
  app.dispatch("htmx:afterRequest", { detail: { successful: true, xhr } });
  assert.deepEqual(pageScans, ["[data-render-time]"], "the response's figure updates the page once");
  const figure = { textContent: "" };
  const footer = { querySelectorAll: (selector) => selector === "[data-render-time]" ? [figure] : [] };
  app.dispatch("htmx:afterSettle", { target: footer, detail: { elt: footer } });
  assert.equal(figure.textContent, "3.1 ms");
  assert.deepEqual(pageScans, ["[data-render-time]"]);
});

test("leaving a page checks its size without counting every element", () => {
  for (const elements of [3999, 4000, 400_000]) {
    const app = page();
    const appended = [];
    Object.assign(app.document, {
      createElement: () => ({ setAttribute() {} }),
      getElementsByTagName: (name) => {
        assert.equal(name, "*");
        // A live collection: indexing walks the page up to that element only.
        return new Proxy({}, {
          get(_, key) {
            if (key === "length") throw new Error("counting every element is the slow part");
            return /^\d+$/.test(key) && Number(key) < elements ? {} : undefined;
          },
        });
      },
    });
    app.document.body.append = (node) => appended.push(node);
    const xhr = { getResponseHeader: () => null };
    app.dispatch("htmx:beforeSwap", { detail: { requestConfig: { boosted: true, verb: "get" }, shouldSwap: true, xhr, serverResponse: "" } });
    assert.equal(appended.length, elements >= 4000 ? 1 : 0, `${elements} elements`);
  }
});

test("visible links come from swapped content instead of a page search after navigating", () => {
  const app = page({ visiblePaths: ["/alice/repo/issues", "/alice/repo/pulls"] });
  const [kept, left] = app.visible;
  assert.deepEqual([...app.observed], [kept, left]);
  const scan = app.document.querySelectorAll;
  app.document.querySelectorAll = (selector) => {
    if (selector.includes("data-preload")) throw new Error("navigating must not search the page for visible links");
    return scan(selector);
  };
  app.dispatch("htmx:load", { detail: { elt: app.document.body } });
  app.navigate("/alice/repo/tree/main");
  left.isConnected = false;
  const next = app.link("/alice/repo/tree/main?page=2", { "data-preload": "visible" });
  const main = { querySelectorAll: (selector) => selector === 'a[data-preload="visible"]' ? [next] : [], matches: () => false };
  app.dispatch("htmx:load", { detail: { elt: main } });
  app.settle();
  assert.deepEqual([...app.observed], [kept, next], "links still on the page stay watched; departed ones are dropped");
});

test("history snapshots over 64k characters are dropped before htmx stores them", () => {
  const app = page();
  const kept = { url: "/alice/repo/issues", content: "x".repeat(64_000) };
  const dropped = { url: "/alice/repo/blob/main/big.rs", content: "x".repeat(64_001) };
  for (const item of [kept, dropped]) app.dispatch("htmx:historyItemCreated", { detail: { item } });
  assert.equal(kept.url, "/alice/repo/issues");
  assert.equal(kept.content.length, 64_000);
  assert.equal(dropped.content, "");
  assert.equal(dropped.url, "\u0000too-large", "the placeholder can never match a real path");
});
