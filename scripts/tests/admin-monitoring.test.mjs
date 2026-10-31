import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/admin-monitoring.js", import.meta.url), "utf8");
const events = () => {
  const listeners = new Map();
  return {
    addEventListener(name, callback) { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name).add(callback); },
    removeEventListener(name, callback) { listeners.get(name)?.delete(callback); },
    emit(name, event) { for (const callback of [...listeners.get(name) ?? []]) callback(event); },
  };
};

function browser(fetch) {
  const timers = new Map();
  let nextTimer = 0;
  const button = { ...events(), setAttribute() {} };
  const status = {};
  const content = { replacements: 0, replaceChildren() { this.replacements++; } };
  const panel = { isConnected: true, querySelector: selector => ({
    "[data-monitoring-toggle]": button, "[data-monitoring-status]": status, "[data-monitoring-content]": content,
  })[selector] };
  const document = { ...events(), hidden: false, querySelectorAll: () => [panel] };
  const window = events();
  const context = {
    document, window, fetch, AbortController,
    DOMParser: class { parseFromString(text) { return { querySelector: () => text === "snapshot" ? {} : null }; } },
    setTimeout(callback, delay) { const id = ++nextTimer; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  };
  vm.runInNewContext(source, context);
  return { panel, document, window, button, status, content, timers, async tick() {
    const [id, timer] = timers.entries().next().value;
    timers.delete(id);
    await timer.callback();
  } };
}
const ok = () => ({ ok: true, status: 200, redirected: false, text: async () => "snapshot" });

test("polls a small uncached fragment once and htmx initialization never duplicates timers", async () => {
  let calls = 0;
  const page = browser(async (url, options) => {
    calls++;
    assert.equal(url, "/admin/monitoring");
    assert.equal(options.cache, "no-store");
    return ok();
  });
  page.document.emit("htmx:afterSettle");
  assert.equal(page.timers.size, 1);
  await page.tick();
  assert.equal(calls, 1);
  assert.equal(page.content.replacements, 1);
  assert.equal([...page.timers.values()][0].delay, 5000);
});

test("pauses when hidden or manually paused and restores without duplicate click handlers", async () => {
  let calls = 0;
  const page = browser(async () => { calls++; return ok(); });
  page.document.hidden = true;
  page.document.emit("visibilitychange");
  assert.equal(page.timers.size, 0);
  page.document.hidden = false;
  page.document.emit("visibilitychange");
  await page.tick();
  assert.equal(calls, 1);
  page.button.emit("click");
  assert.equal(page.status.textContent, "Paused");
  assert.equal(page.timers.size, 0);
  page.window.emit("pagehide");
  page.window.emit("pageshow");
  assert.equal(page.timers.size, 1);
  page.button.emit("click");
  assert.equal(page.button.textContent, "Resume");
  assert.equal(page.timers.size, 0);
});

test("failures retain data and back off; expired access stops polling", async () => {
  const page = browser(async () => { throw new Error("Offline"); });
  await page.tick();
  assert.equal(page.content.replacements, 0);
  assert.equal(page.status.textContent, "Connection lost · retrying");
  assert.equal([...page.timers.values()][0].delay, 10000);
  const denied = browser(async () => ({ ok: false, status: 404 }));
  await denied.tick();
  assert.equal(denied.button.disabled, true);
  assert.equal(denied.timers.size, 0);
});

test("navigation aborts pending requests and prevents stale writes", async () => {
  let finish;
  let signal;
  const page = browser((url, options) => {
    signal = options.signal;
    return new Promise(resolve => { finish = resolve; });
  });
  const pending = page.tick();
  page.document.emit("htmx:beforeSwap", { detail: { shouldSwap: true, target: { contains: () => true } } });
  assert.equal(signal.aborted, true);
  finish(ok());
  await pending;
  assert.equal(page.content.replacements, 0);
  assert.equal(page.timers.size, 0);
});
