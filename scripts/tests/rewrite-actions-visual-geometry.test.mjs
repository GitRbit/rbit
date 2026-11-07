import assert from "node:assert/strict";
import test from "node:test";
import { geometryEvidence, captureGeometry, assertCaptureGeometry, normalizeCaptureOrigin } from "../../devtools/actions-visual/capture-geometry.mjs";

const hidden = { scrollX: 0, scrollY: 154, skipTop: -100, skipBottom: -52, mainFocused: true, skipFocused: false };

test("capture geometry retains only bounded finite numbers and booleans", () => {
  assert.deepEqual(geometryEvidence({ ...hidden, url: "private", text: "secret" }), hidden);
  const invalid = geometryEvidence({ scrollX: Infinity, scrollY: "private", skipTop: NaN, skipBottom: 10000001,
    mainFocused: "true", skipFocused: "false" });
  assert.deepEqual(invalid, { scrollX: null, scrollY: null, skipTop: null, skipBottom: null, mainFocused: null, skipFocused: null });
  assert.doesNotMatch(JSON.stringify(invalid), /private|secret/);
  assert.equal(geometryEvidence({ scrollY: 12.3456 }).scrollY, 12.35);
});

test("keyboard success also requires the entire unfocused skip link outside the viewport", () => {
  assertCaptureGeometry(hidden);
  for (const change of [{ mainFocused: false }, { skipFocused: true }, { skipBottom: 2 },
    { skipTop: 12, skipBottom: 50 }, { skipTop: null }, { skipBottom: null }, { skipTop: -10, skipBottom: -20 }]) {
    assert.throws(() => assertCaptureGeometry({ ...hidden, ...change }));
  }
});

test("geometry observation neither scrolls nor replaces the existing focus test", async () => {
  let evaluations = 0;
  const result = await captureGeometry({ evaluate: async () => { evaluations++; return hidden; } });
  assert.equal(evaluations, 1);
  assert.deepEqual(result, hidden);
  assertCaptureGeometry(result);
});

test("capture origin scrolls once and retains the already-proved keyboard focus", async () => {
  const calls = [];
  const result = await normalizeCaptureOrigin({ evaluate: async callback => {
    calls.push(callback.toString());
    return calls.length === 1 ? undefined : { ...hidden, scrollY: 0 };
  } });
  assert.equal(calls.length, 2);
  assert.match(calls[0], /window\.scrollTo\(0, 0\)/);
  assert.doesNotMatch(calls.join(""), /\.focus\(|\.blur\(|\.style/);
  assert.equal(result.mainFocused, true);
  assert.equal(result.scrollY, 0);
});

test("capture origin refuses lost focus or incomplete scrolling", async () => {
  for (const change of [{ mainFocused: false }, { scrollX: 1 }, { scrollY: 142 }]) {
    let evaluations = 0;
    await assert.rejects(normalizeCaptureOrigin({ evaluate: async () => ++evaluations === 1
      ? undefined : { ...hidden, scrollY: 0, ...change } }));
  }
});
