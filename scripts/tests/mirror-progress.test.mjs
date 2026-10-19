import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/mirror-progress.js", import.meta.url), "utf8");

function browser(fetch) {
  const fields = new Map(["phase", "message", "indicator"].map(name => [`[data-mirror-${name}]`, {}]));
  const panel = { isConnected: true, dataset: { statusUrl: "/a/b/mirror/status", homeUrl: "/a/b" }, querySelector: selector => fields.get(selector) };
  const timers = [];
  const redirects = [];
  const listeners = new Map();
  const document = { querySelectorAll: () => [panel], addEventListener: (event, callback) => listeners.set(event, callback) };
  const window = { location: { replace: url => redirects.push(url) }, addEventListener() {} };
  vm.runInNewContext(source, { document, window, fetch, WeakSet, setTimeout: (callback, delay) => timers.push({ callback, delay }) });
  return { panel, fields, timers, redirects, listeners };
}

const settle = () => new Promise(resolve => setImmediate(resolve));
const response = status => ({ ok: true, redirected: false, json: async () => status });

test("mirror progress reports worker stages then reloads the finished repository", async () => {
  let done = false;
  const page = browser(async () => response(done ? { complete: true } : { phase: "fetching", message: "Downloading Git objects", complete: false }));
  await settle();
  assert.equal(page.fields.get("[data-mirror-phase]").textContent, "fetching");
  assert.equal(page.fields.get("[data-mirror-indicator]").hidden, false);
  assert.equal(page.timers[0].delay, 1500);
  done = true;
  await page.timers.shift().callback();
  assert.deepEqual(page.redirects, ["/a/b"]);
  assert.equal(page.timers.length, 0);
});

test("failed mirrors show diagnostics and continue checking for an administrator retry", async () => {
  const page = browser(async () => response({ phase: "failed", message: "Fetch refused", complete: false }));
  await settle();
  assert.equal(page.fields.get("[data-mirror-message]").textContent, "Fetch refused");
  assert.equal(page.fields.get("[data-mirror-indicator]").hidden, true);
  assert.equal(page.timers[0].delay, 10000);
  assert.deepEqual(page.redirects, []);
});

test("network failures reconnect and detached pages do not navigate", async () => {
  let requests = 0;
  const page = browser(async () => { requests++; throw new Error("Offline"); });
  await settle();
  assert.match(page.fields.get("[data-mirror-message]").textContent, /Reconnecting/);
  page.panel.isConnected = false;
  await page.timers.shift().callback();
  assert.equal(requests, 1);
  assert.deepEqual(page.redirects, []);
});

test("htmx settle does not create duplicate polling loops", async () => {
  let requests = 0;
  const page = browser(async () => { requests++; return response({ phase: "queued", message: "Waiting", complete: false }); });
  page.listeners.get("htmx:afterSettle")();
  await settle();
  assert.equal(requests, 1);
});
