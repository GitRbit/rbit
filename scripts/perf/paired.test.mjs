import assert from "node:assert/strict";
import test from "node:test";
import { comparePairs, pagePairs } from "./paired.mjs";

test("equal variants are not flagged and intervals straddle no change", () => {
  const pairs = Array.from({ length: 40 }, (_, i) => [10 + (i % 7) * 0.1, 10 + ((i + 3) % 7) * 0.1]);
  const result = comparePairs(pairs);
  assert.equal(result.flagged, false);
  assert.ok(result.changeInterval[0] <= 0 && result.changeInterval[1] >= 0);
  assert.deepEqual(comparePairs(pairs), result, "the bootstrap is deterministic");
});

test("a clear slowdown beyond five percent is flagged, a small one is not", () => {
  const base = Array.from({ length: 40 }, (_, i) => 10 + (i % 5) * 0.05);
  const slow = comparePairs(base.map((value) => [value, value * 1.2]));
  assert.equal(slow.flagged, true);
  assert.ok(Math.abs(slow.change - 0.2) < 1e-9);
  assert.ok(slow.changeInterval[0] > 0.15);
  assert.equal(comparePairs(base.map((value) => [value, value * 1.03])).flagged, false);
});

test("pairs match by round and refuse incomplete reports", () => {
  const sample = (variant, round, totalMs) => ({ phase: "measured", page: "p", variant, round, totalMs });
  const report = { valid: true, config: { rounds: 3, variants: [{ name: "a" }, { name: "b" }], pages: [{ name: "p" }] },
    samples: [sample("b", 1, 2), sample("a", 1, 1), sample("a", 2, 3), sample("b", 2, 4), sample("a", 3, 5), { ...sample("b", 3, 6), phase: "warmup" }] };
  assert.deepEqual(pagePairs([report], "totalMs").get("p"), { names: ["a", "b"], pairs: [[1, 2], [3, 4]] });
  assert.throws(() => pagePairs([{ ...report, valid: false }], "totalMs"), /valid/);
  const swapped = { ...report, config: { ...report.config, variants: [{ name: "b" }, { name: "a" }] } };
  assert.deepEqual(pagePairs([report, swapped], "totalMs", "a").get("p").pairs, [[1, 2], [3, 4], [1, 2], [3, 4]], "a reference name pools either listing order");
  assert.throws(() => pagePairs([report], "totalMs", "c"), /no variant/);
});
