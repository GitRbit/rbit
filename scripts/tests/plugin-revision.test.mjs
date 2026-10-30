import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const navigation = await readFile(new URL("../../crates/rbit-web/assets/nav.js", import.meta.url), "utf8");
const source = navigation.slice(navigation.indexOf("// Runtime feature changes"), navigation.indexOf("// Title-Case"));

function page(revision) {
  let listener;
  let invalidated = 0;
  const history = [];
  const redirects = [];
  vm.runInNewContext(source, {
    document: { body: { dataset: { pluginRevision: revision } }, addEventListener(name, callback) { assert.equal(name, "htmx:beforeOnLoad"); listener = callback; } },
    clearNavigationResponses() { invalidated += 1; },
    localStorage: { removeItem(key) { history.push(key); } },
    location: { href: "https://git.example/alice/rocket", assign(url) { redirects.push(url); } },
  });
  return {
    history, redirects, invalidated: () => invalidated,
    receive(next) {
      let prevented = false;
      listener({ preventDefault() { prevented = true; }, detail: { xhr: { responseURL: "https://git.example/alice/rocket/actions", getResponseHeader: () => next } } });
      return prevented;
    },
  };
}

test("runtime revision changes discard navigation and history before refreshing the shell", () => {
  const app = page("1");
  assert.equal(app.receive("2"), true);
  assert.equal(app.invalidated(), 1);
  assert.deepEqual(app.history, ["htmx-history-cache"]);
  assert.deepEqual(app.redirects, ["https://git.example/alice/rocket/actions"]);
});

test("unchanged or absent revision metadata keeps normal navigation fast", () => {
  const app = page("1");
  assert.equal(app.receive("1"), false);
  assert.equal(app.receive(null), false);
  assert.equal(app.invalidated(), 0);
  assert.deepEqual(app.redirects, []);
});
