import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/actions-run-layout.js", import.meta.url), "utf8");

/** Runs the module against a small document; `narrow` is the phone media query's answer. */
function page({ narrow = false, now = 1_000, clock = {}, elements = [], navs = [] } = {}) {
  const listeners = new Map(), media = new Map();
  let interval = null;
  const document = {
    readyState: "complete",
    addEventListener: (type, listener) => listeners.set(type, listener),
    querySelector: selector => (selector === "[data-run-clock]" ? clock : null),
    querySelectorAll: selector => (selector === "[data-since]" ? elements : selector === "details[data-run-nav]" ? navs : []),
  };
  const query = { matches: narrow, addEventListener: (type, listener) => media.set(type, listener) };
  const context = {
    document, window: { matchMedia: () => query }, WeakMap, Number, Math, String,
    Date: { now: () => now * 1000 }, setInterval: callback => { interval = callback; },
  };
  vm.runInNewContext(source, context);
  return { tick: () => interval(), query, media, setNow: value => { now = value; } };
}

const clock = (serverNow, patterns = ["11h 22m", "33m 44s", "55s"]) =>
  ({ dataset: { runClock: String(serverNow), clockHm: patterns[0], clockMs: patterns[1], clockS: patterns[2] } });

test("running durations tick from the server's clock in the page's shapes", () => {
  const job = { dataset: { since: "900" }, textContent: "1m 40s" };
  const run = { dataset: { since: "-2720" }, textContent: "" };
  const odd = { dataset: { since: "x" }, textContent: "keep" };
  // The browser's clock runs 500 s behind the server's.
  const view = page({ now: 500, clock: clock(1_000), elements: [job, run, odd] });
  view.tick();
  assert.equal(job.textContent, "1m 40s");
  assert.equal(run.textContent, "1h 2m");
  assert.equal(odd.textContent, "keep");
  view.setNow(549);
  view.tick();
  assert.equal(job.textContent, "2m 29s");
  view.setNow(8_920);
  view.tick();
  assert.equal(job.textContent, "2h 22m", "values equal to the marks stay put");
});

test("durations keep each language's shape", () => {
  const job = { dataset: { since: "990" }, textContent: "" };
  page({ now: 1_000, clock: clock(1_000, ["11時間22分", "33分44秒", "55秒"]), elements: [job] }).tick();
  assert.equal(job.textContent, "10秒");
});

test("the navigation folds on phones once and opens beside the page", () => {
  const nav = { open: true, dataset: {} };
  const view = page({ narrow: true, navs: [nav] });
  assert.equal(nav.open, false);
  nav.open = true;
  view.query.matches = true;
  view.media.get("change")();
  assert.equal(nav.open, false, "a phone again folds it");
  view.query.matches = false;
  view.media.get("change")();
  assert.equal(nav.open, true);
});
