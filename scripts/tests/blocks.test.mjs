import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../crates/rbit-web/assets/blocks.js", import.meta.url), "utf8");

function mount() {
  const listeners = new Map();
  const scans = [];
  const scope = (name) => ({
    name,
    querySelectorAll(selector) {
      scans.push(name);
      return [];
    },
    matches: () => false,
  });
  const document = Object.assign(scope("document"), {
    body: scope("body"),
    addEventListener(type, listener) {
      listeners.set(type, [...(listeners.get(type) ?? []), listener]);
    },
  });
  vm.runInNewContext(source, {
    document,
    Element: Object,
    CSS: { supports: () => true },
    ResizeObserver: class { observe() {} },
    WeakMap,
    WeakSet,
  });
  const dispatch = (type, elt) => {
    for (const listener of listeners.get(type) ?? []) listener({ detail: { elt } });
  };
  return { document, scans, dispatch, scope };
}

test("the loaded page is sized once, not again when htmx announces it", () => {
  const page = mount();
  assert.deepEqual(page.scans, ["document"]);
  page.dispatch("htmx:load", page.document.body);
  assert.deepEqual(page.scans, ["document"]);
});

test("swapped and restored content is sized within the new element only", () => {
  const page = mount();
  page.dispatch("htmx:load", page.document.body);
  const main = page.scope("main");
  const tabs = page.scope("tabs");
  page.dispatch("htmx:load", main);
  page.dispatch("htmx:load", tabs);
  page.dispatch("htmx:historyRestore", page.document.body);
  assert.deepEqual(page.scans, ["document", "main", "tabs"]);
});

test("a body swap after startup is still sized", () => {
  const page = mount();
  page.dispatch("htmx:load", page.document.body);
  page.dispatch("htmx:load", page.document.body);
  assert.deepEqual(page.scans, ["document", "body"]);
});
