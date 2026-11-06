import assert from "node:assert/strict";
import test from "node:test";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { compilerIdentity } from "../render-compile/compiler-identity.mjs";
import { OwnedMemory } from "../render-compile/memory.mjs";
import { sampleValidity } from "../render-compile/identity.mjs";

const privateRoot = join(process.cwd(), "private-compiler-location");
const binary = Buffer.from("compiler fixture");
const dependencies = {
  run: (_name, args) => { assert.deepEqual(args, ["--print", "sysroot"]); return privateRoot; },
  canonical: (path) => path,
  stat: () => ({ isFile: () => true }),
  read: () => binary,
};

test("sysroot compiler identity remains private and supports an effective custom driver", () => {
  for (const driver of ["rustc", join(privateRoot, "custom-driver.exe")]) {
    const identity = compilerIdentity({ checkout: "." }, { RUSTC: driver }, dependencies);
    assert.equal(identity.provenance.verifiedCandidate, true);
    assert.equal(identity.provenance.customDriver, driver !== "rustc");
    assert.equal(identity.provenance.binarySha256, createHash("sha256").update(binary).digest("hex"));
    assert.equal(identity.observerArguments()[0], "-ExpectedCompilerPath");
    assert.ok(identity.observerArguments()[1].startsWith(privateRoot));
    assert.ok(!JSON.stringify(identity).includes("private-compiler-location"));
    assert.ok(!JSON.stringify(identity.provenance).includes(driver === "rustc" ? "private-compiler-location" : driver));
  }
});

test("missing/nonstandard compiler and invalid sysroot fail closed without publishing errors", () => {
  for (const override of [
    { run: () => { throw new Error(privateRoot); } },
    { run: () => "relative/sysroot" },
    { run: () => `${privateRoot}\nsecond-line` },
    { stat: () => ({ isFile: () => false }) },
    { read: () => { throw new Error(privateRoot); } },
  ]) {
    const identity = compilerIdentity({}, { RUSTC: "custom-driver" }, { ...dependencies, ...override });
    assert.equal(identity.provenance.verifiedCandidate, false);
    assert.deepEqual(identity.observerArguments(), []);
    assert.equal(identity.provenance.reason, "custom-rustc-memory-identity-unverified");
    assert.ok(!JSON.stringify(identity).includes("private-compiler-location"));
  }
});

const root = { pid: 1, parentPid: 0, beforeMs: 100, afterMs: 101 };
const cargo = { pid: 1, parentPid: 0, createdMs: 100, kind: "cargo", workingSetBytes: 10, privateBytes: 10 };
function rustc(pid, parentPid, actualCompilerMatch, createdMs = 105) {
  return { pid, parentPid, createdMs, kind: "rustc", crateName: "rbit_render_probe", actualCompilerMatch, workingSetBytes: pid * 100, privateBytes: pid * 100 };
}
function validity(memory) {
  return sampleValidity([{ package: "rbit-render-probe", targetKinds: ["lib"], fresh: false, features: ["blocks", "count-8"] }],
    { mode: "blocks", count: 8 }, { code: 0 }, { fingerprint: "same" }, { fingerprint: "same" }, memory);
}
function observe(memory, rows) { memory.observe({ startMs: 110, endMs: 120, rows }); }

test("proxy carrying identical crate arguments preserves timing but cannot establish usable compiler memory", () => {
  const memory = new OwnedMemory(root);
  observe(memory, [cargo, rustc(2, 1, false)]);
  const report = memory.finish();
  assert.equal(report.perProcessSampledPeaks[1].crateName, "rbit_render_probe");
  assert.equal(report.perProcessSampledPeaks[1].actualCompilerMatch, false);
  assert.equal(validity(report).compileComparable, true);
  assert.equal(validity(report).memoryComparable, false);
  assert.ok(validity(report).memoryReasons.includes("probe-library-actual-compiler-not-observed"));
  // Both proxy and real rustc remain visible separately; no sum or silent pruning.
  observe(memory, [cargo, rustc(2, 1, false), rustc(3, 2, true, 106)]);
  const actual = memory.finish();
  assert.equal(actual.perProcessSampledPeaks.length, 3);
  assert.equal(validity(actual).memoryComparable, true);
});

test("legacy evidence and custom compiler mismatches are unsuitable without changing timing validity", () => {
  const legacy = { reasons: [], perProcessSampledPeaks: [{ kind: "rustc", crateName: "rbit_render_probe" }] };
  assert.equal(validity(legacy).compileComparable, true);
  assert.equal(validity(legacy).memoryComparable, false);
  const memory = new OwnedMemory(root);
  observe(memory, [cargo, rustc(2, 1, false)]);
  const report = { ...memory.finish(), compilerIdentity: { customDriver: true, verifiedCandidate: true } };
  assert.ok(validity(report).memoryReasons.includes("custom-rustc-memory-identity-unverified"));
});

test("reused or externally owned PIDs cannot establish actual probe identity", () => {
  const memory = new OwnedMemory(root);
  observe(memory, [cargo, rustc(2, 1, false)]);
  observe(memory, [{ ...cargo, parentPid: 99, createdMs: 110 }, rustc(2, 99, true, 111)]);
  const report = memory.finish();
  assert.equal(report.perProcessSampledPeaks.some((row) => row.actualCompilerMatch), false);
  assert.equal(validity(report).memoryComparable, false);
});
