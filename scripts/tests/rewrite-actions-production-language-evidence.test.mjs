import assert from "node:assert/strict";
import test from "node:test";
import { languageProbe, languageRecord } from "../../devtools/actions-visual/production-language-evidence.mjs";

test("language evidence retains only bounded member vocabulary and sanitized observations", () => {
  const stages = []; let report;
  const observe = languageProbe("refreshed", value => stages.push(value), value => { report = languageRecord(report, value); });
  observe("status", 500, 200);
  observe("content-language", "PRIVATE_HEADER_SENTINEL\nhttps://example.invalid/token", "en");
  observe("url", false, true);
  observe("document-language", null, "en");
  observe("selected-language", "es", "en");
  observe("document-marker", false, true);
  assert.equal(report.length, 6);
  assert.equal(report[1].observed, "other");
  assert.equal(report[3].observed, "missing");
  assert.equal(report[5].observed, false);
  assert.doesNotMatch(JSON.stringify({ report, stages }), /PRIVATE|https|token|example/);
  assert.ok(Buffer.byteLength(JSON.stringify({ report, stages })) < 2048);
  observe("status", 200, 200);
  assert.equal(report.length, 6);
  assert.equal(report.at(-1).observed, 200);
  const spanish = languageProbe("spanish", () => {}, value => { report = languageRecord(report, value); });
  spanish("status", 200, 200);
  assert.equal(report.length, 1, "old navigation phase is not retained");
});

test("report admission refuses unbounded stages, extra fields and unsanitized values", () => {
  assert.throws(() => languageProbe("private-url", () => {}, () => {}), /LANGUAGE_DIAGNOSTIC_REFUSED/);
  const observe = languageProbe("refreshed", () => {}, () => {});
  assert.throws(() => observe("cookie", "private", true), /LANGUAGE_DIAGNOSTIC_REFUSED/);
  assert.throws(() => observe("document-language", "en", "private"), /LANGUAGE_DIAGNOSTIC_REFUSED/);
  const value = { phase: "refreshed", member: "status", observed: 200, expected: 200 };
  for (const invalid of [{ ...value, path: "/private" }, { ...value, observed: Infinity },
    { ...value, observed: "raw-header" }, { ...value, phase: "other" }, { ...value, member: "other" }]) {
    assert.throws(() => languageRecord([], invalid), /LANGUAGE_DIAGNOSTIC_REFUSED/);
  }
  assert.throws(() => languageRecord(Array(7).fill(value), value), /LANGUAGE_DIAGNOSTIC_REFUSED/);
});
