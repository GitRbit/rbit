import assert from "node:assert/strict";
import test from "node:test";
import { delta, histogramQuantile, MIX, parseArgs, parseMetrics, pick, poolReport, random, total } from "./contention.mjs";

const exposition = (acquired, busy) => `# TYPE rbit_sqlite_acquire_wait_seconds histogram
rbit_sqlite_acquire_wait_seconds_bucket{mode="query",le="0.0001"} ${acquired[0]}
rbit_sqlite_acquire_wait_seconds_bucket{mode="query",le="0.001"} ${acquired[1]}
rbit_sqlite_acquire_wait_seconds_bucket{mode="query",le="+Inf"} ${acquired[2]}
rbit_sqlite_acquire_wait_seconds_sum{mode="query"} ${acquired[2] * 0.0002}
rbit_sqlite_acquire_wait_seconds_count{mode="query"} ${acquired[2]}
rbit_sqlite_busy_errors_total{store="issues",phase="transaction",code="busy_snapshot"} ${busy}
rbit_http_requests_total{method="GET",route="/{owner}/{repo}",status="5xx"} 1
`;

test("arguments are validated and default to three levels", () => {
  const options = parseArgs(["run", "http://127.0.0.1:3031/"]);
  assert.equal(options.origin, "http://127.0.0.1:3031");
  assert.deepEqual(options.levels, [4, 16, 64]);
  assert.deepEqual(parseArgs(["run", "http://127.0.0.1:3031", "--levels", "2,8"]).levels, [2, 8]);
  for (const argv of [
    ["bench", "http://127.0.0.1:3031"],
    ["run", "http://example.com"],
    ["run", "http://127.0.0.1:3031/path"],
    ["run", "http://127.0.0.1:3031", "--levels", "0"],
    ["run", "http://127.0.0.1:3031", "--seconds"],
    ["run", "http://127.0.0.1:3031", "--unknown", "1"],
  ]) {
    assert.throws(() => parseArgs(argv), argv.join(" "));
  }
});

test("the operation sequence is deterministic and follows the weights", () => {
  const a = random(7);
  const b = random(7);
  const first = Array.from({ length: 5 }, () => a());
  assert.deepEqual(first, Array.from({ length: 5 }, () => b()));
  const next = random(42);
  const counts = {};
  for (let i = 0; i < 20_000; i++) {
    const name = pick(MIX, next());
    counts[name] = (counts[name] ?? 0) + 1;
  }
  for (const [name, weight] of MIX) assert.ok(Math.abs(counts[name] / 20_000 - weight / 100) < 0.02, `${name}: ${counts[name]}`);
  assert.equal(pick(MIX, 0), MIX[0][0]);
  assert.equal(pick(MIX, 0.999_999), MIX.at(-1)[0]);
});

test("metric changes across a level are reported per label", () => {
  const before = parseMetrics(exposition([10, 10, 10], 1));
  const after = parseMetrics(exposition([10, 100, 110], 4));
  const change = delta(before, after);
  assert.equal(total(change, "rbit_sqlite_acquire_wait_seconds_count", { mode: "query" }), 100);
  // 90 of the 100 new waits fell in (0.1 ms, 1 ms], 10 above it.
  assert.ok(Math.abs(histogramQuantile(change, "rbit_sqlite_acquire_wait_seconds", 0.5, { mode: "query" }) - 0.000_6) < 1e-9);
  assert.equal(histogramQuantile(change, "rbit_sqlite_acquire_wait_seconds", 0.95, { mode: "query" }), 0.001);
  const report = poolReport(change);
  assert.equal(report.acquireQuery.count, 100);
  assert.equal(report.busyErrors, 3);
  assert.deepEqual(report.busy, { "issues/transaction/busy_snapshot": 3 });
  assert.equal(report.serverErrors, 0, "unchanged counters are not counted");
  assert.equal(report.writeLockWait.count, 0);
  assert.equal(report.writeLockWait.p95Ms, null);
});
