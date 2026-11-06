import assert from "node:assert/strict";
import test from "node:test";
import { productionChecks } from "../../devtools/actions-visual/production-checks.mjs";
import { productionOrigin, productionRun, containerMode } from "../../devtools/actions-visual/production-policy.mjs";

function fixtureModel({ loseDraft = false, keepRevokedForm = false } = {}) {
  let writer = true;
  const contexts = [], captures = [], stages = [];
  const hasForm = () => writer || keepRevokedForm;
  const fixture = { authenticate: async () => {}, healthy: () => {},
    revokeWriter: async () => { writer = false; } };
  const browser = { newContext: async options => {
    const context = { options, closed: false, setDefaultTimeout: () => {}, setDefaultNavigationTimeout: () => {}, addCookies: async () => {},
      route: async () => {}, routeWebSocket: async () => {}, on: () => {}, addInitScript: async () => {},
      close: async () => { context.closed = true; } };
    contexts.push(context);
    context.newPage = async () => {
      let handle = { isConnected: true, dispose: async () => {} }, checked = false, focused = false;
      const checkbox = { check: async () => { checked = true; }, focus: async () => { focused = true; },
        isChecked: async () => checked, evaluate: async () => focused, isVisible: async () => hasForm() };
      const form = { count: async () => hasForm() ? 1 : 0,
        getAttribute: async name => name === "method" ? "post" : `${productionRun}/approve`, locator: () => checkbox };
      const page = {
        goto: async url => {
          assert.equal(url, productionOrigin + productionRun);
          return { status: () => 200, allHeaders: async () => ({ "content-security-policy": "script-src 'self'" }) };
        },
        evaluate: async operation => operation.toString().includes("scrollWidth") ? false : 0,
        locator: selector => {
          if (selector.includes("input[name=")) return checkbox;
          if (selector.startsWith("form[")) return form;
          if (selector === "#run h1") return { count: async () => 1 };
          assert.equal(selector, "#run");
          return { elementHandle: async () => handle, getAttribute: async name => name === "lang"
            ? options.extraHTTPHeaders["Accept-Language"] : options.extraHTTPHeaders["Accept-Language"] === "ar-XB" ? "rtl" : "ltr" };
        },
        waitForFunction: async (predicate, previous) => {
          previous.isConnected = false;
          handle = { isConnected: true, dispose: async () => {} };
          if (loseDraft) { checked = false; focused = false; }
          assert.equal(predicate(previous), true);
        },
      };
      return page;
    };
    return context;
  } };
  return { browser, fixture, contexts, captures, stages,
    run: () => productionChecks(browser, fixture, async (_page, id) => { captures.push(id); }, stage => { stages.push(stage); }) };
}

test("the contained mode has no caller-selected URL, command or mount flags", () => {
  assert.equal(containerMode([]), "prototype"); assert.equal(containerMode(["--production"]), "production");
  for (const args of [["--url", "https://private.test"], ["--production", "extra"], ["--no-sandbox"], ["--mount", "/private"]]) {
    assert.throws(() => containerMode(args), /MODE_REFUSED/);
  }
});

test("production checks require native no-JS controls, real-poll drafts and subsequent permission removal", async () => {
  const model = fixtureModel(); await model.run();
  assert.deepEqual(model.captures, ["production-run-desktop-light", "production-run-mobile-nojs",
    "production-run-mobile-rtl-dark", "production-run-mobile-revoked"]);
  assert.equal(model.contexts.length, 3);
  assert.equal(model.contexts[1].options.javaScriptEnabled, false);
  assert.equal(model.contexts[2].options.viewport.width, 360);
  assert.equal(model.contexts[2].options.extraHTTPHeaders["Accept-Language"], "ar-XB");
  assert.ok(model.contexts.every(context => context.closed && context.options.serviceWorkers === "block"));
  assert.equal(model.stages.at(-1), "permission-removal");
});

test("lost drafts or retained revoked controls fail instead of being captured as accepted", async () => {
  const lost = fixtureModel({ loseDraft: true });
  await assert.rejects(lost.run(), { code: "ERR_ASSERTION" });
  assert.equal(lost.contexts.length, 1); assert.equal(lost.contexts[0].closed, true);
  const revoked = fixtureModel({ keepRevokedForm: true });
  await assert.rejects(revoked.run(), { code: "ERR_ASSERTION" });
  assert.equal(revoked.captures.includes("production-run-mobile-revoked"), false);
  assert.ok(revoked.contexts.every(context => context.closed));
});
