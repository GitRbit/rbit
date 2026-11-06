import assert from "node:assert/strict";
import test from "node:test";
import { installRenderTiming } from "../../crates/rbit-web/assets/render-time.js";

function fixture(performance) {
  const footer = { textContent: "stale cached value" };
  const listeners = new Map();
  const document = {
    querySelectorAll: () => [footer],
    addEventListener(name, listener) {
      const callbacks = listeners.get(name) ?? [];
      callbacks.push(listener); listeners.set(name, callbacks);
    },
  };
  installRenderTiming(document, performance);
  return { document, footer, listeners,
    emit(name, detail) { for (const listener of listeners.get(name) ?? []) listener({ detail }); } };
}

const navigation = duration => ({ getEntriesByType: () => [{ serverTiming: [{ name: "render", duration }] }] });

test("fresh navigation timing replaces the footer reused by HTTP revalidation", () => {
  const page = fixture(navigation(0.5));
  assert.equal(page.footer.textContent, "500 µs");
  installRenderTiming(page.document, navigation(999));
  assert.equal(page.footer.textContent, "500 µs");
  assert.equal(page.listeners.get("htmx:afterRequest").length, 1);
});

test("missing or invalid navigation metrics preserve server-rendered text", () => {
  for (const performance of [{}, navigation(NaN), navigation(Infinity), navigation(-1),
    { getEntriesByType() { throw new Error("unavailable"); } }]) {
    assert.equal(fixture(performance).footer.textContent, "stale cached value");
  }
});

test("successful HTMX timing follows navigation and is restored only in the swapped subtree", () => {
  const page = fixture(navigation(0.5));
  page.emit("htmx:afterRequest", { successful: true, xhr: { getResponseHeader: () => "database;dur=2, render;dur=12.34" } });
  assert.equal(page.footer.textContent, "12.3 ms");
  const swapped = { textContent: "old body footer" }, untouched = { textContent: "unrelated" };
  page.emit("htmx:afterSettle", { elt: { querySelectorAll: () => [swapped] } });
  assert.equal(swapped.textContent, "12.3 ms");
  assert.equal(untouched.textContent, "unrelated");
});

test("errors and malformed timing cannot replace the last valid measurement", () => {
  const page = fixture(navigation(0.5));
  page.emit("htmx:afterRequest", { successful: false, xhr: { getResponseHeader: () => "render;dur=900" } });
  for (const value of [null, "render;dur=NaN", "render;dur=-2", "render;dur=1.2.3", "render;dur=Infinity"]) {
    page.emit("htmx:afterRequest", { successful: true, xhr: { getResponseHeader: () => value } });
  }
  assert.equal(page.footer.textContent, "500 µs");
  page.emit("htmx:afterRequest", { successful: true, xhr: { getResponseHeader: () => "render;dur=1234.5" } });
  assert.equal(page.footer.textContent, "1.23 s");
});
