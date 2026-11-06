// Numeric evidence for the fixed fixture's viewport before full-page capture.
import assert from "node:assert/strict";

export function geometryEvidence(value) {
  const numeric = number => typeof number === "number" && Number.isFinite(number) && Math.abs(number) <= 10000000
    ? Math.round(number * 100) / 100 : null;
  return {
    scrollX: numeric(value?.scrollX), scrollY: numeric(value?.scrollY),
    skipTop: numeric(value?.skipTop), skipBottom: numeric(value?.skipBottom),
    skipFocused: typeof value?.skipFocused === "boolean" ? value.skipFocused : null,
    mainFocused: typeof value?.mainFocused === "boolean" ? value.mainFocused : null,
  };
}

export async function captureGeometry(page) {
  return geometryEvidence(await page.evaluate(() => {
    const skip = document.querySelector(".skip-link");
    const rectangle = skip?.getBoundingClientRect();
    return { scrollX: window.scrollX, scrollY: window.scrollY,
      skipTop: rectangle?.top, skipBottom: rectangle?.bottom,
      skipFocused: skip === document.activeElement,
      mainFocused: document.querySelector("#main") === document.activeElement };
  }));
}

export function assertCaptureGeometry(evidence) {
  assert.equal(evidence.mainFocused, true, "MAIN_FOCUS_LOST");
  assert.equal(evidence.skipFocused, false, "SKIP_LINK_STILL_FOCUSED");
  assert.ok(evidence.skipTop !== null && evidence.skipBottom !== null
    && evidence.skipTop <= evidence.skipBottom && evidence.skipBottom <= 0, "SKIP_LINK_VIEWPORT_VISIBLE");
}
