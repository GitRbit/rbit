import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const navigation = await readFile(new URL("../../crates/rbit-web/assets/nav.js", import.meta.url), "utf8");
const source = navigation.slice(navigation.indexOf("// Runtime feature changes"), navigation.indexOf("// Title-Case"));

function page(revision, locale = "en:0", previousLocale = null) {
  let listener;
  let invalidated = 0;
  const history = [];
  const redirects = [];
  vm.runInNewContext(source, {
    document: { body: { dataset: { pluginRevision: revision, localeRevision: locale } }, addEventListener(name, callback) { assert.equal(name, "htmx:beforeOnLoad"); listener = callback; } },
    clearNavigationResponses() { invalidated += 1; },
    localStorage: { getItem() { return previousLocale; }, setItem() {}, removeItem(key) { history.push(key); } },
    location: { href: "https://git.example/alice/rocket", assign(url) { redirects.push(url); } },
  });
  return {
    history, redirects, invalidated: () => invalidated,
    receive(next, locale = "en:0", boosted = true) {
      let prevented = false;
      listener({ preventDefault() { prevented = true; }, detail: { requestConfig: { boosted, verb: "get" }, xhr: { responseURL: "https://git.example/alice/rocket/actions", getResponseHeader: (name) => name === "Rbit-Locale-Revision" ? locale : next } } });
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


test("locale or catalog changes discard stale shell and history even when plugins are unchanged", () => {
  for (const locale of ["fr:1", "en:2"]) {
    const app = page("1", "en:1");
    assert.equal(app.receive("1", locale), true);
    assert.equal(app.invalidated(), 1);
    assert.deepEqual(app.history, ["htmx-history-cache"]);
  }
});


test("full navigation after a language form retires persisted history on the new document", () => {
  const app = page("1", "fr:2", "en:1");
  assert.equal(app.invalidated(), 1);
  assert.deepEqual(app.history, ["htmx-history-cache"]);
  assert.deepEqual(app.redirects, [], "the new document already has the selected locale");
});


test("a locale change observed by a fragment poll refreshes the current full page", () => {
  const app = page("1", "en:1");
  assert.equal(app.receive("1", "fr:2", false), true);
  assert.deepEqual(app.redirects, ["https://git.example/alice/rocket"]);
});
