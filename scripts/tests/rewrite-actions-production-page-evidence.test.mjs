import assert from "node:assert/strict";
import test from "node:test";
import { checkPage } from "../../devtools/actions-visual/production-page.mjs";
import { refreshedPageProbe, pageRecord } from "../../devtools/actions-visual/production-page-evidence.mjs";

function fixture({ overflow = false, csp = 0, faults = [] } = {}) {
  const stages = []; let evidence;
  const page = { evaluate: async operation => operation.toString().includes("scrollWidth") ? overflow : csp };
  return { stages, evidence: () => evidence,
    run: javascript => checkPage(page, faults, javascript, refreshedPageProbe(value => stages.push(value),
      value => { evidence = pageRecord(evidence, value); })) };
}

test("original page assertions still fail independently after recording their exact member", async () => {
  for (const [member, values, expected] of [
    ["overflow", { overflow: true }, true], ["csp", { csp: 2 }, 2],
    ["browser-faults", { faults: ["console-error"] }, { count: 1, categories: ["console-error"], truncated: false }],
  ]) {
    const model = fixture(values); await assert.rejects(model.run(), assert.AssertionError);
    assert.equal(model.stages.at(-1), `language-refreshed-page-${member}`);
    assert.deepEqual(model.evidence().at(-1), { member, observed: expected });
  }
  let missing;
  // Explicitly missing init-script counters remain failures; no diagnostic default masks them.
  const page = { evaluate: async operation => operation.toString().includes("scrollWidth") ? false : undefined };
  await assert.rejects(checkPage(page, [], true, refreshedPageProbe(() => {}, value => { missing = value; })), assert.AssertionError);
  assert.deepEqual(missing, { member: "csp", observed: "missing" });
});

test("successful and no-JS checks preserve their original assertion selection", async () => {
  const model = fixture(); await model.run();
  assert.deepEqual(model.evidence().map(item => item.member), ["overflow", "csp", "browser-faults"]);
  const noJs = fixture({ csp: 99 }); await noJs.run(false);
  assert.deepEqual(noJs.evidence().map(item => item.member), ["overflow", "browser-faults"]);
});

test("browser diagnostics are bounded enum/count evidence, never private payloads", () => {
  let evidence;
  const observe = refreshedPageProbe(() => {}, value => { evidence = pageRecord(evidence, value); });
  observe("overflow", "PRIVATE_VALUE"); observe("csp", undefined);
  observe("browser-faults", ["console-error", ...Array(1000).fill("https://private.invalid/?cookie=SECRET")]);
  assert.deepEqual(evidence[0], { member: "overflow", observed: "other" });
  assert.deepEqual(evidence[1], { member: "csp", observed: "missing" });
  assert.deepEqual(evidence[2], { member: "browser-faults", observed: { count: 1001, categories: ["console-error", "other"], truncated: true } });
  assert.ok(Buffer.byteLength(JSON.stringify(evidence)) < 1024);
  assert.doesNotMatch(JSON.stringify(evidence), /PRIVATE|https|cookie|SECRET/);
  observe("csp", 0); assert.equal(evidence.length, 3);
});

test("record admission refuses unknown members, raw categories and extra private fields", () => {
  const observe = refreshedPageProbe(() => {}, () => {});
  assert.throws(() => observe("url", "PRIVATE_URL"), /PAGE_DIAGNOSTIC_REFUSED/);
  for (const value of [{ member: "csp", observed: Infinity }, { member: "overflow", observed: "private" },
    { member: "overflow", observed: false, cookie: "secret" },
    { member: "browser-faults", observed: { count: 1, categories: ["raw private error"], truncated: false } }]) {
    assert.throws(() => pageRecord([], value), /PAGE_DIAGNOSTIC_REFUSED/);
  }
  assert.throws(() => pageRecord(Array(4).fill({ member: "csp", observed: 0 }), { member: "csp", observed: 0 }), /PAGE_DIAGNOSTIC_REFUSED/);
});
