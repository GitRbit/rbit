import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { sampler } from "../render-compile/sampler.mjs";
import { sampleCounts } from "../render-compile/summary.mjs";
import { OwnedMemory } from "../render-compile/memory.mjs";
import { sampleValidity } from "../render-compile/identity.mjs";

const config = { memory: "windows", "sample-ms": 50 };
function fake() {
  const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
  child.kill = () => {}; child.unref = () => {};
  child.ready = () => child.stdout.write('{"kind":"ready"}\n');
  child.stop = (code = 0) => { child.stdout.end(); child.stderr.end(); child.emit("close", code, null); };
  return child;
}
const deps = (child, extra = {}) => ({ spawnObserver: () => child, startupMs: 20, stopMs: 5, killMs: 50, writeStop: async () => {}, ...extra });

test("kill escalation waits for close and trailing stderr drain before finalizing", async () => {
  const child = fake(); queueMicrotask(child.ready);
  const observer = await sampler("control", config, deps(child));
  let stopped = false;
  child.kill = () => {
    setTimeout(() => { child.stderr.write("trailing data"); child.emit("close", 0, null); }, 5);
    setTimeout(() => { child.stdout.end(); child.stderr.end(); stopped = true; }, 15);
  };
  const report = await observer.finish();
  assert.equal(stopped, true); assert.equal(report.observerStopped, true);
  assert.equal(report.observerStderrSha256, createHash("sha256").update("trailing data").digest("hex"));
  assert.ok(report.reasons.includes("observer-stop-timeout"));
  assert.strictEqual(await observer.finish(), report);
});

test("unproved closure rejects, retains control evidence and leaves late stderr safe", async () => {
  const child = fake(); queueMicrotask(child.ready);
  const observer = await sampler("control", config, deps(child, { killMs: 5 }));
  await assert.rejects(observer.finish(), (error) => {
    assert.equal(error.retainControl, true); assert.equal(error.observerReport.observerStopped, false);
    assert.equal(error.observerReport.observerStderrSha256, null); return true;
  });
  assert.doesNotThrow(() => child.stderr.write("arrived after failed shutdown")); child.stop();
});

test("spawn error, error frame, early close and timeout never admit a compiler", async () => {
  for (const start of [child => child.emit("error", new Error("failed")), child => child.stdout.write('{"kind":"error"}\n'), child => child.stop(), () => {}]) {
    const child = fake(); let compiled = false;
    queueMicrotask(() => start(child));
    child.kill = () => child.stop();
    await assert.rejects((async () => { await sampler("control", config, deps(child)); compiled = true; })(), /Observer did not become ready/);
    assert.equal(compiled, false);
  }
});

test("startup failure with an unkillable observer retains evidence", async () => {
  const child = fake(); queueMicrotask(() => child.emit("error", new Error("failed")));
  await assert.rejects(sampler("control", config, deps(child, { killMs: 5 })), (error) => error.retainControl === true);
  child.stop();
});

test("summary distinguishes valid timing from entirely missed memory", () => {
  const summary = sampleCounts();
  summary.observe({ kind: "warmup" });
  summary.observe({ kind: "sample", validity: { compileComparable: true, memoryComparable: false } });
  summary.observe({ kind: "sample", validity: { compileComparable: false, memoryComparable: false } });
  assert.deepEqual(summary.counts, { samples: 2, compileComparable: 1, compileInvalid: 1, memoryComparable: 0, memoryInvalid: 2 });
});

test("observing only build-script rustc cannot prove probe library memory", () => {
  const memory = new OwnedMemory({ pid: 1, parentPid: 0, beforeMs: 100, afterMs: 101 });
  const row = (pid, parentPid, createdMs, kind, crateName) => ({ pid, parentPid, createdMs, kind, crateName, workingSetBytes: 100, privateBytes: 100 });
  memory.observe({ startMs: 110, endMs: 120, rows: [row(1, 0, 100, "cargo"), row(2, 1, 101, "rustc", "build_script_build")] });
  const artifacts = [{ package: "rbit-render-probe", targetKinds: ["lib"], fresh: false, features: ["blocks", "count-8"] }];
  const validity = sampleValidity(artifacts, { mode: "blocks", count: 8 }, { code: 0 }, { fingerprint: "x" }, { fingerprint: "x" }, memory.finish());
  assert.equal(validity.compileComparable, true); assert.equal(validity.memoryComparable, false);
  assert.ok(validity.memoryReasons.includes("probe-library-rustc-not-observed"));
  const summary = sampleCounts(); summary.observe({ kind: "sample", validity });
  assert.equal(summary.counts.compileComparable, 1); assert.equal(summary.counts.memoryComparable, 0);
  memory.observe({ startMs: 130, endMs: 140, rows: [row(1, 0, 100, "cargo"), row(3, 1, 125, "rustc", "rbit_render_probe")] });
  assert.equal(sampleValidity(artifacts, { mode: "blocks", count: 8 }, { code: 0 }, { fingerprint: "x" }, { fingerprint: "x" }, memory.finish()).memoryComparable, true);
});

test("Windows provider queries arguments only for creation-filtered owned rustc", { skip: process.platform !== "win32" }, () => {
  const output = execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("render-observer-fixture.ps1", import.meta.url))], { encoding: "utf8", windowsHide: true });
  assert.equal(output.trim(), "Query policy validated");
});
