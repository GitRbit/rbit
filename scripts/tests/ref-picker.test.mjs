import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = (await readFile(new URL("../../crates/rbit-web/assets/app.js", import.meta.url), "utf8"))
  .replace(/^import .*;$/gm, "");

function harness() {
  const listeners = new Map();
  const requests = [];
  const menus = [];
  class Details {
    open = false;
    unloaded = true;
    picker = true;
    matches() { return this.picker; }
    querySelector() { return this.unloaded ? {} : null; }
    removeAttribute(name) { if (name === "open") this.open = false; }
  }
  const document = {
    addEventListener(name, callback) { listeners.set(name, [...(listeners.get(name) ?? []), callback]); },
    querySelectorAll() { return menus.filter(menu => menu.open); },
    querySelector() { return null; },
  };
  const window = { htmx: { trigger(element, name) { requests.push({element, name}); } } };
  vm.runInNewContext(source, { document, window, navigator: {}, HTMLDetailsElement: Details, ResizeObserver: class { observe() {} unobserve() {} } });
  const picker = new Details();
  menus.push(picker);
  return { picker, requests, window, emit: (name, detail = {}) => listeners.get(name).forEach(callback => callback({target: picker, detail})) };
}

test("refs load only on open; failed loads can retry and successful loads are reused", () => {
  const h = harness();
  assert.equal(h.requests.length, 0);
  h.emit("toggle");
  assert.equal(h.requests.length, 0);
  h.picker.open = true;
  h.emit("toggle");
  assert.equal(h.requests.length, 1);
  h.picker.open = false;
  h.emit("toggle");
  h.picker.open = true;
  h.emit("toggle");
  assert.equal(h.requests.length, 2);
  h.picker.unloaded = false;
  h.emit("toggle");
  assert.equal(h.requests.length, 2);
});

test("fragment swaps keep the menu open, boosted navigation closes it", () => {
  const h = harness();
  h.picker.open = true;
  h.emit("htmx:afterSettle", {requestConfig: {boosted: false}});
  assert.equal(h.picker.open, true);
  h.emit("htmx:afterSettle", {requestConfig: {boosted: true}});
  assert.equal(h.picker.open, false);
});

test("other details and a missing htmx runtime do not request refs", () => {
  const h = harness();
  h.picker.open = true;
  h.picker.picker = false;
  h.emit("toggle");
  assert.equal(h.requests.length, 0);
  h.picker.picker = true;
  delete h.window.htmx;
  h.emit("toggle");
  assert.equal(h.requests.length, 0);
});
