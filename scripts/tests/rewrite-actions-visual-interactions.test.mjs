import assert from "node:assert/strict";
import test from "node:test";
import { interactionStep, InteractionFailure } from "../../devtools/actions-visual/interaction-evidence.mjs";
import { interactionChecks } from "../../devtools/actions-visual/interaction-checks.mjs";
import { context as prototypeContext, render } from "../../docs/design/prototypes/actions/render.mjs";
import { allowedRequest } from "../../devtools/actions-visual/policy.mjs";

test("the actual live-log interaction navigation selects the fixture that owns simulated output", async () => {
  let navigation, closed = false;
  await assert.rejects(interactionChecks(async () => ({ faults: [], context: {
    newPage: async () => ({ goto: async url => { navigation = url; throw Error("intentional fixture-only stop"); } }),
    close: async () => { closed = true; },
  } })), error => error.evidence.stage === "javascript.job-navigation" && error.evidence.code === "OPERATION_FAILED");
  assert.equal(closed, true);
  assert.equal(allowedRequest(navigation, "GET"), true);
  const url = new URL(navigation);
  assert.equal(url.pathname, "/job"); assert.equal(url.searchParams.get("state"), "running");
  for (const [query, simulate] of [[new URLSearchParams(), false], [url.searchParams, true]]) {
    const model = prototypeContext("/job", query), html = render(model);
    assert.equal(model.state, simulate ? "running" : "failed");
    assert.equal(html.includes("data-simulate"), simulate);
    const lines = [...html.matchAll(/<li id="(L\d+)"/g)].map(match => match[1]);
    assert.equal(lines.length, 80); assert.equal(lines.at(-1), "L50000");
  }
});

test("interaction diagnostics retain only fixed stages and categories, never raw page or browser text", async () => {
  for (const [error, expected] of [
    [Object.assign(new Error("secret path/token"), { code: "ERR_ASSERTION" }), "ASSERTION_FAILED"],
    [new Error("Timeout 5000ms exceeded at private/path"), "TIMEOUT"],
    [new Error("Target page, context or browser has been closed secret"), "TARGET_CLOSED"],
    [new Error("strict mode violation: private locator text"), "STRICT_SELECTOR"],
    [new Error("net::ERR_ABORTED at private URL"), "NAVIGATION_ABORTED"],
    [new Error("Protocol error secret"), "PROTOCOL_ERROR"],
    [new Error("secret path/token"), "OPERATION_FAILED"],
  ]) {
    await assert.rejects(interactionStep("logs.paused-scroll", () => { throw error; }), failure => {
      assert.ok(failure instanceof InteractionFailure);
      assert.deepEqual(failure.evidence, { stage: "logs.paused-scroll", code: expected });
      assert.doesNotMatch(JSON.stringify(failure), /secret|private|token/);
      return true;
    });
  }
  await assert.rejects(interactionStep("private-path", () => { throw Error("secret"); }), error => {
    assert.equal(error.evidence.stage, "unknown"); return true;
  });
});

test("nested checks preserve the original failure identity and successful return values", async () => {
  const original = new InteractionFailure("logs.follow-last-line", { code: "ERR_ASSERTION" });
  await assert.rejects(interactionStep("javascript.page", () => { throw original; }), error => error === original);
  assert.equal(await interactionStep("logs.follow-count", async () => 80), 80);
});

test("failed navigation awaits context cleanup and cleanup failure cannot erase its diagnosis", async () => {
  let release, closeStarted = false, settled = false, created = 0;
  const barrier = new Promise(resolve => { release = resolve; });
  const promise = interactionChecks(async () => {
    created++;
    return { faults: [], context: {
      newPage: async () => ({ goto: async () => { throw Error("Timeout 10000ms exceeded private URL"); } }),
      close: async () => { closeStarted = true; await barrier; throw Error("cleanup private path"); },
    } };
  });
  promise.then(() => { settled = true; }, () => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(closeStarted, true); assert.equal(settled, false);
  release();
  await assert.rejects(promise, error => {
    assert.deepEqual(error.evidence, { stage: "javascript.job-navigation", code: "TIMEOUT" });
    assert.doesNotMatch(JSON.stringify(error), /private/);
    return true;
  });
  assert.equal(created, 1, "no next context starts after an invalid interaction");
});

test("context creation failure receives a bounded phase without trying to close an unowned context", async () => {
  await assert.rejects(interactionChecks(async () => { throw Error("private spawn failure"); }), error => {
    assert.deepEqual(error.evidence, { stage: "javascript.context", code: "OPERATION_FAILED" });
    return true;
  });
});
