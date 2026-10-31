import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/admin-restart.js", import.meta.url), "utf8");
const settle = () => new Promise(resolve => setImmediate(resolve));
function browser(fetch) {
  const timers = new Map();
  const events = new Map();
  const status = { isConnected: true };
  const redirects = [];
  let nextTimer = 0;
  const document = { hidden: false, querySelector: selector => selector === "[data-restart-status]" ? status : null, addEventListener() {} };
  vm.runInNewContext(source, {
    document, fetch, AbortController,
    window: { location: { replace: path => redirects.push(path) }, addEventListener: (name, callback) => events.set(name, callback) },
    setTimeout(callback, delay) { const id = ++nextTimer; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  return { timers, status, events, redirects, document, async tick() { const [id, timer] = timers.entries().next().value; timers.delete(id); await timer.callback(); } };
}
const response = restarting => ({ ok: true, status: 200, json: async () => ({ restarting }) });

test("only the new application's ready response reconnects to the dashboard", async () => {
  let pending = true;
  const page = browser(async (url, options) => {
    assert.equal(url, "/admin/restart/status");
    assert.equal(options.cache, "no-store");
    return response(pending);
  });
  await settle();
  assert.deepEqual(page.redirects, []);
  pending = false;
  await page.tick();
  assert.deepEqual(page.redirects, ["/admin"]);
  assert.equal(page.timers.size, 0);
});

function confirmation(fetch) {
  const events = new Map();
  const button = { disabled: false };
  const link = { textContent: "Cancel" };
  const messages = [];
  const heading = { textContent: "Restart server" };
  const timers = new Map();
  let replaced = false;
  let nextTimer = 0;
  const section = { querySelectorAll: () => [] };
  const status = { isConnected: true, closest: () => section };
  const form = {
    isConnected: true,
    action: "http://localhost/admin/restart",
    querySelector: selector => selector === "a" ? link : button,
    addEventListener: (name, callback) => events.set(name, callback),
    closest: () => ({ replaceWith: incoming => { assert.equal(incoming, section); replaced = true; } }),
    append: message => messages.push(message),
  };
  const document = {
    hidden: false, addEventListener() {},
    querySelector: selector => selector === 'form[action="/admin/restart"]' ? form : selector === "h2" ? heading : null,
    createElement: () => ({ setAttribute() {} }),
  };
  vm.runInNewContext(source, {
    document, fetch, AbortController, URLSearchParams,
    FormData: class { *[Symbol.iterator]() { yield ["confirmation", "restart"]; } },
    DOMParser: class { parseFromString() { return { querySelector: () => status }; } },
    window: { location: { replace() {} }, addEventListener() {} },
    setTimeout(callback, delay) { const id = ++nextTimer; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  return { form, button, link, messages, heading, timers, get replaced() { return replaced; },
    submit: () => events.get("submit")({ preventDefault() {} }) };
}

test("confirmation keeps loaded reconnect logic in the document across shutdown", async () => {
  const requests = [];
  const page = confirmation(async (url, options) => {
    requests.push([url, options]);
    if (options.method === "POST") {
      assert.equal(options.body.toString(), "confirmation=restart");
      return { status: 202, text: async () => "accepted page" };
    }
    return response(true);
  });
  await page.submit();
  assert.equal(page.replaced, true);
  assert.equal(page.heading.textContent, "Restarting server");
  assert.equal(page.button.disabled, true);
  assert.equal(requests.length, 2, "POST is followed by a status GET without a document navigation or script download");
  await page.submit();
  assert.equal(requests.length, 2, "duplicate submissions are ignored");
});

test("unconfirmed responses never repeat a POST or claim the restart succeeded", async () => {
  let requests = 0;
  const page = confirmation(async () => { requests++; throw new Error("Connection lost"); });
  await page.submit();
  assert.equal(page.replaced, false);
  assert.equal(page.timers.size, 0);
  assert.match(page.messages[0].textContent, /could not be confirmed/);
  assert.equal(page.link.textContent, "Return to dashboard");
  await page.submit();
  assert.equal(requests, 1);
});

test("a late restart response cannot change a page the administrator navigated to", async () => {
  let finish;
  const page = confirmation(() => new Promise(resolve => { finish = resolve; }));
  const submitted = page.submit();
  page.form.isConnected = false;
  finish({ status: 202, text: async () => "accepted page" });
  await submitted;
  assert.equal(page.replaced, false);
  assert.equal(page.heading.textContent, "Restart server");
  assert.equal(page.timers.size, 0);
});

test("expected downtime retries and changed sessions stop reconnecting", async () => {
  const page = browser(async () => { throw new Error("Server restarting"); });
  await settle();
  assert.equal([...page.timers.values()][0].delay, 3000);
  assert.deepEqual(page.redirects, []);
  const denied = browser(async () => ({ status: 404, ok: false }));
  await settle();
  assert.equal(denied.timers.size, 0);
  assert.match(denied.status.textContent, /session or permissions changed/);
});

test("leaving the reconnect page aborts pending work and prevents a late redirect", async () => {
  let finish;
  let signal;
  const page = browser((url, options) => { signal = options.signal; return new Promise(resolve => { finish = resolve; }); });
  page.events.get("pagehide")();
  assert.equal(signal.aborted, true);
  finish(response(false));
  await settle();
  assert.deepEqual(page.redirects, []);
  assert.equal(page.timers.size, 0);
});
