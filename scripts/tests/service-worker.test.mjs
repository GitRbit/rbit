import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/sw.js", import.meta.url), "utf8");
const origin = "https://git.example.com";
const asset = `${origin}/assets/app.js?v=0123456789ab`;

function worker(cacheControl = "public, max-age=31536000, immutable", { brokenStorage = false, brokenWrite = false, networkDown = false, brokenRefresh = false } = {}) {
  const listeners = new Map();
  const stored = new Map();
  const deleted = [];
  const installed = [];
  let fetches = 0;
  const cache = {
    async add(request) {
      installed.push(request);
      if (brokenRefresh && installed.length > 1) throw new Error("Refresh unavailable");
      stored.set(request.url, new Response("offline"));
    },
    async delete(request) { return stored.delete(new URL(request, origin).href); },
    async match(request) { return stored.get(typeof request === "string" ? new URL(request, origin).href : request.url)?.clone(); },
    async put(request, response) {
      if (brokenWrite) throw new Error("Quota exceeded");
      stored.set(request.url, response);
    },
  };
  class WorkerRequest extends Request {
    constructor(url, options) { super(new URL(url, origin), options); }
  }
  vm.runInNewContext(source, {
    URL,
    Request: WorkerRequest,
    Response,
    self: {
      location: { origin },
      addEventListener(name, listener) { listeners.set(name, listener); },
      skipWaiting() {},
      registration: { navigationPreload: { async enable() {} } },
      clients: { async claim() {} },
    },
    caches: {
      async open() {
        if (brokenStorage) throw new Error("Storage unavailable");
        return cache;
      },
      async keys() { return ["rbit-static-old", "rbit-static-__VERSION__", "other-app"]; },
      async delete(key) { deleted.push(key); },
    },
    async fetch() {
      fetches += 1;
      if (networkDown) throw new Error("Network unavailable");
      return new Response("javascript", { headers: { "Cache-Control": cacheControl } });
    },
  });
  return {
    stored, deleted, installed,
    get fetches() { return fetches; },
    async dispatch(name, request) {
      let response;
      const pending = [];
      listeners.get(name)({
        request,
        data: request,
        respondWith(value) { response = value; },
        waitUntil(value) { pending.push(value); },
      });
      const result = await response;
      await Promise.all(pending);
      return result;
    },
  };
}

test("only current immutable asset responses enter the service worker cache", async () => {
  const app = worker();
  assert.equal(await (await app.dispatch("fetch", new Request(asset))).text(), "javascript");
  await app.dispatch("fetch", new Request(asset));
  assert.equal(app.fetches, 1, "an immutable asset is reused");
  for (const policy of ["no-cache", "no-store", "private, immutable", "public, immutable, no-cache"]) {
    const app = worker(policy);
    await app.dispatch("fetch", new Request(asset));
    await app.dispatch("fetch", new Request(asset));
    assert.equal(app.fetches, 2, `${policy} must reach the network`);
    assert.equal(app.stored.size, 0);
  }
});

test("unversioned and malformed asset URLs use normal HTTP caching", async () => {
  const app = worker();
  for (const suffix of ["", "?v=old", "?v=0123456789ab&random=1"]) {
    assert.equal(await app.dispatch("fetch", new Request(`${origin}/assets/app.js${suffix}`)), undefined);
  }
  assert.equal(app.fetches, 0, "the worker does not intercept these requests");
});

test("cache storage failures do not break healthy network responses", async () => {
  for (const options of [{ brokenStorage: true }, { brokenWrite: true }]) {
    const app = worker(undefined, options);
    assert.equal(await (await app.dispatch("fetch", new Request(asset))).text(), "javascript");
  }
});

test("installation omits cookies and activation preserves other applications' caches", async () => {
  const app = worker();
  await app.dispatch("install");
  assert.equal(app.installed[0].credentials, "omit");
  assert.equal(app.installed[0].url, `${origin}/offline`);
  await app.dispatch("activate");
  assert.deepEqual(app.deleted, ["rbit-static-old"]);
});

test("offline navigation uses the current cache and survives a missing fallback", async () => {
  const navigation = { method: "GET", mode: "navigate", url: `${origin}/private/repo` };
  const app = worker(undefined, { networkDown: true });
  await app.dispatch("install");
  assert.equal(await (await app.dispatch("fetch", navigation)).text(), "offline");
  app.stored.clear();
  assert.equal((await app.dispatch("fetch", navigation)).status, 503);
  const noStorage = worker(undefined, { networkDown: true, brokenStorage: true });
  assert.equal((await noStorage.dispatch("fetch", navigation)).status, 503);
});


test("locale changes refresh anonymous offline HTML while retaining immutable assets", async () => {
  const app = worker();
  await app.dispatch("install");
  await app.dispatch("fetch", new Request(asset));
  assert.ok(app.stored.has(`${origin}/offline`));
  await app.dispatch("message", { type: "rbit-locale-changed" });
  assert.equal(app.stored.has(`${origin}/offline`), true);
  assert.equal(app.installed.length, 2);
  assert.equal(app.installed[1].credentials, "omit");
  assert.equal(app.installed[1].cache, "reload");
  assert.ok(app.stored.has(asset));
});


test("a failed locale refresh retains the previously available offline document", async () => {
  const app = worker(undefined, { brokenRefresh: true });
  await app.dispatch("install");
  app.stored.set(`${origin}/offline`, new Response("previous offline"));
  await app.dispatch("message", { type: "rbit-locale-changed" });
  assert.equal(await app.stored.get(`${origin}/offline`).text(), "previous offline");
});
