import assert from "node:assert/strict";
import test from "node:test";
import { options, plan, cargoArgs, cleanArgs, measurementEnv, MODES, COUNTS } from "../render-compile/options.mjs";
import { sequence } from "../render-compile/sequence.mjs";
import { OwnedMemory } from "../render-compile/memory.mjs";
import { artifactIdentity, sampleValidity, flagIdentity } from "../render-compile/identity.mjs";

const cli = ["--command", "check", "--profile", "dev", "--jobs", "1", "--output", "report.jsonl"];
test("only explicit probe command/profile/job policy is accepted", () => {
  const config = options(cli, "linux");
  for (const args of [[], [...cli, "--package", "other"], [...cli, "--target-dir", "/tmp"], cli.map((v) => v === "check" ? "clean" : v), [...cli, "--rounds", "7"], [...cli, "--jobs", "0"]]) assert.throws(() => options(args, "linux"));
  assert.deepEqual(cleanArgs(config), ["clean", "--package", "rbit-render-probe", "--profile", "dev", "--locked"]);
  assert.deepEqual(cargoArgs({ mode: "blocks", count: 96 }, config), ["--package", "rbit-render-probe", "--lib", "--locked", "--no-default-features", "--features", "blocks,count-96", "--profile", "dev", "--jobs", "1"]);
  assert.equal(measurementEnv({ RUSTC_WRAPPER: "cache" }).RUSTC_WRAPPER, "");
});

test("six rounds balance each mode in each position for every count", () => {
  const jobs = plan(6); assert.equal(jobs.length, 72);
  for (const count of COUNTS) for (const mode of MODES) for (let position = 0; position < 3; position++) assert.equal(jobs.filter((job) => job.count === count && job.mode === mode && job.position === position).length, 2);
});

test("all dependency warmups precede serial measurements; failure stops later work", async () => {
  const calls = []; let active = 0;
  await sequence({ rounds: 6 }, async (job, phase) => { assert.equal(active++, 0); await Promise.resolve(); calls.push({ job, phase }); active--; });
  assert.equal(calls.length, 84); assert.ok(calls.slice(0, 12).every((call) => call.phase === "warmup"));
  assert.ok(calls.slice(12).every((call) => call.phase === "sample"));
  let count = 0; await assert.rejects(sequence({ rounds: 6 }, async () => { if (++count === 14) throw new Error("child failed"); })); assert.equal(count, 14);
});

const root = { pid: 10, parentPid: 1, beforeMs: 1000, afterMs: 1010 };
const row = (pid, parentPid, createdMs, kind, bytes = 10, extra = {}) => ({ pid, parentPid, createdMs, kind, workingSetBytes: bytes, privateBytes: bytes, ...extra });
const frame = (...rows) => ({ startMs: 1100, endMs: 1200, rows });
test("owned tree peaks stay per-process across nonconcurrent rustc lifetimes", () => {
  const memory = new OwnedMemory(root);
  memory.observe(frame(row(10, 1, 1005, "cargo"), row(11, 10, 1050, "rustc", 100, { crateName: "probe", targetTriple: "x86_64-pc-windows-msvc" })));
  memory.observe(frame(row(10, 1, 1005, "cargo"), row(12, 10, 1150, "rustc", 200)));
  const report = memory.finish();
  assert.deepEqual(report.perProcessSampledPeaks.filter((v) => v.kind === "rustc").map((v) => v.workingSetBytes), [100, 200]);
  assert.equal(report.concurrentTreePeakBytes, null); assert.equal(report.completeLifetimeCoverage, false); assert.deepEqual(report.reasons, []);
  assert.equal(report.perProcessSampledPeaks[1].crateName, "probe");
});

test("PID reuse and stale parent IDs cannot grant ownership; private tokens never survive", () => {
  const memory = new OwnedMemory(root);
  memory.observe(frame(row(10, 1, 1005, "cargo")));
  memory.observe(frame(row(10, 1, 1150, "other"), row(12, 10, 1160, "rustc", 999, { crateName: "C:/secret", commandLine: "credential" })));
  const report = memory.finish();
  assert.ok(report.reasons.includes("root-pid-reused")); assert.ok(report.reasons.includes("external-compiler-observed"));
  assert.ok(!JSON.stringify(report).includes("credential")); assert.ok(!JSON.stringify(report).includes("secret"));
  assert.equal(report.perProcessSampledPeaks.some((v) => v.pid === 12), false);
});

test("invalid census and missing short-lived compilers remain explicit", () => {
  const memory = new OwnedMemory(root); memory.observe({ startMs: 5, endMs: 4, rows: [] });
  const report = memory.finish(); assert.ok(report.reasons.includes("invalid-sampler-frame")); assert.ok(report.reasons.includes("cargo-root-not-observed"));
  assert.ok(report.reasons.includes("rustc-not-observed-short-lived-processes-may-be-missed"));
});

function artifact(pkg = "rbit-render-probe", fresh = false) {
  return { reason: "compiler-artifact", package_id: `path+file:///private/credential#${pkg}@0.1.0`, features: ["blocks", "count-8"], target: { kind: ["lib"], src_path: "/private/path" }, profile: { opt_level: "0", secret: "credential" }, fresh };
}
test("artifact provenance sanitizes paths and rejects mixed work or reused probe", () => {
  const probe = artifactIdentity(artifact()); const job = { mode: "blocks", count: 8 }; const source = { fingerprint: "same" }; const memory = { reasons: [] };
  assert.ok(!JSON.stringify(probe).includes("private")); assert.ok(!JSON.stringify(probe).includes("credential"));
  assert.equal(sampleValidity([probe], job, { code: 0 }, source, source, memory).compileComparable, true);
  for (const artifacts of [[{ ...probe, fresh: true }], [probe, artifactIdentity(artifact("askama"))], [{ ...probe, features: ["blocks", "count-8", "inheritance"] }]]) assert.equal(sampleValidity(artifacts, job, { code: 0 }, source, source, memory).compileComparable, false);
  assert.equal(sampleValidity([probe], job, { code: 101 }, source, source, memory).compileComparable, false);
  assert.equal(sampleValidity([probe], job, { code: 0 }, source, { fingerprint: "changed" }, memory).compileComparable, false);
  assert.equal(sampleValidity([probe], job, { code: 0 }, source, source, { reasons: ["cargo-root-not-observed"] }).memoryComparable, false);
  assert.equal(artifactIdentity({ ...artifact(), profile: { opt_level: "3", debuginfo: null } }).profile.debuginfo, null);
  assert.equal(artifactIdentity({ ...artifact(), fresh: undefined }).invalid, true);
});

test("environment identity is insertion-order independent and contains no values", () => {
  const a = flagIdentity({ RUSTFLAGS: "secret", PATH: "/private" }); const b = flagIdentity({ PATH: "/private", RUSTFLAGS: "secret" });
  assert.deepEqual(a, b); assert.ok(!JSON.stringify(a).includes("secret")); assert.ok(!JSON.stringify(a).includes("private"));
});

