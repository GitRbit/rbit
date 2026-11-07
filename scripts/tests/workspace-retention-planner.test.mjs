import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeGraph } from "../workspace/retention/graph.mjs";
import { planRetention } from "../workspace/retention/planner.mjs";
const policy = { compatibility: "a".repeat(20), manifestDigest: "b".repeat(64), configurationDigest: "c".repeat(64) };
const request = args => ({ command: "test", args, policy });
const original = normalizeGraph(request(["-pcore"])), replacement = normalizeGraph(request(["-pweb"]));
function observation(extra = {}) {
  return { lease: { state: "owned", tokenVerified: true, processTreeCertain: true }, requested: replacement,
    current: { id: "current", bytes: 120, lastUsed: 10, successful: true, graph: original, handoff: { complete: true, exact: true } },
    retired: [], usage: { complete: true, namespaceBytes: 120, freeBytes: 500 }, budgetBytes: 100,
    pendingCompaction: true, protection: { runtimeCopiesIndependent: true, pendingHandoffsSafe: true },
    measurements: { coldCostMeasured: true, estimatedCandidateBytes: 80, reserveBytes: 20 }, ...extra };
}
function candidate(extra = {}) {
  return { id: "candidate", bytes: 60, lastUsed: 20, graph: replacement, status: "succeeded", executionComplete: true,
    handoff: { complete: true, exact: true }, ...extra };
}

test("completed overage records pending work but rotation is disabled by default", () => {
  const pending = planRetention(observation({ phase: "after-success", pendingCompaction: false }));
  assert.equal(pending.kind, "mark-pending"); assert.equal(pending.pendingCompaction, true);
  const disabled = planRetention(observation());
  assert.equal(disabled.reason, "ROTATION_DISABLED"); assert.equal(disabled.apply, false);
  const under = observation(); under.current.bytes = 80; under.usage.namespaceBytes = 80;
  assert.equal(planRetention(under).reason, "UNDER_BUDGET");
  assert.equal(planRetention(observation({ budgetBytes: 0 })).reason, "BUDGET_DISABLED");
});

test("same oversized graph, including different test filters, never schedules another cold generation", () => {
  for (const args of [["-pcore", "one"], ["--package=core", "two", "--", "--nocapture"], ["-pcore", "--locked"]]) {
    const result = planRetention(observation({ requested: normalizeGraph(request(args)), rotationEnabled: true }));
    assert.equal(result.kind, "reuse"); assert.equal(result.reason, "OVERSIZED_SAME_GRAPH");
    assert.equal(result.overageBytes, 20); assert.equal(result.discardIds, undefined);
  }
});

test("material graph replacement requires measured cold cost/headroom and preserves current until success", () => {
  const ready = planRetention(observation({ rotationEnabled: true }));
  assert.equal(ready.kind, "prepare-candidate");
  assert.deepEqual(ready.retainIds, ["current"]); assert.deepEqual(ready.discardIds, []);
  assert.equal(ready.maximumCandidates, 1); assert.equal(ready.maximumRetiredAfterPromotion, 1);
  assert.equal(ready.requiredFreeBytes, 100); assert.equal(ready.apply, false);
  for (const [extra, expected] of [
    [{ measurements: undefined }, "COLD_COST_OR_HEADROOM_UNMEASURED"],
    [{ usage: { complete: true, namespaceBytes: 120, freeBytes: 99 } }, "INSUFFICIENT_HEADROOM"],
    [{ usage: { complete: true, namespaceBytes: 120, freeBytes: null } }, "INSUFFICIENT_HEADROOM"],
    [{ pendingCompaction: false }, "COMPLETED_OVERAGE"],
    [{ requested: normalizeGraph(request(["--unrecognized-graph-flag"])) }, "UNKNOWN_GRAPH"],
    [{ requested: normalizeGraph({ ...request(["-pweb"]), env: { CARGO_PROFILE_TEST_DEBUG: "0" } }) }, "COMPATIBILITY_POLICY_CHANGED"],
  ]) assert.equal(planRetention(observation({ rotationEnabled: true, ...extra })).reason, expected);
});

test("failed candidates cannot replace successful output or accumulate another candidate", () => {
  const result = planRetention(observation({ phase: "candidate-complete", candidate: candidate({ status: "failed" }),
    usage: { complete: true, namespaceBytes: 180, freeBytes: 500 } }));
  assert.equal(result.kind, "discard-candidate"); assert.equal(result.currentId, "current");
  assert.deepEqual(result.discardIds, ["candidate"]); assert.deepEqual(result.retainIds, ["current"]);
  const busy = planRetention(observation({ candidate: candidate({ status: "building" }), usage: { complete: true, namespaceBytes: 180, freeBytes: 500 } }));
  assert.equal(busy.reason, "CANDIDATE_IN_PROGRESS");
  assert.equal(planRetention(observation({ candidates: [candidate(), candidate({ id: "second" })] })).reason, "GENERATION_BOUND_EXCEEDED");
});

test("promotion and retired eviction wait for exact handoff and keep at most one warm lane", () => {
  const base = observation({ phase: "candidate-complete", rotationEnabled: true, candidate: candidate(), budgetBytes: 200,
    retired: [{ id: "retired", bytes: 30, lastUsed: 5, successful: true }],
    usage: { complete: true, namespaceBytes: 210, freeBytes: 500 } });
  for (const handoff of [undefined, { complete: false, exact: true }, { complete: true, exact: false }]) {
    const result = planRetention({ ...base, candidate: candidate({ handoff }) });
    assert.equal(result.reason, "EXACT_PUBLICATION_PENDING"); assert.equal(result.discardIds, undefined);
  }
  const promoted = planRetention(base);
  assert.equal(promoted.currentId, "candidate"); assert.equal(promoted.retiredId, "current");
  assert.deepEqual(promoted.discardIds, ["retired"]); assert.equal(promoted.retainedBytes, 180);
  const smallBudget = planRetention({ ...base, budgetBytes: 100 });
  assert.equal(smallBudget.retiredId, "retired"); assert.deepEqual(smallBudget.discardIds, ["current"]);
  const oversized = planRetention({ ...base, budgetBytes: 100, candidate: candidate({ bytes: 250 }),
    usage: { complete: true, namespaceBytes: 400, freeBytes: 500 } });
  assert.equal(oversized.retiredId, null); assert.equal(oversized.overageBytes, 150);
  assert.deepEqual(oversized.discardIds, ["current", "retired"]);
});

test("retired whole-lane cleanup preserves unattributed bytes and requires successful publication", () => {
  const base = observation({ phase: "after-success", rotationEnabled: true,
    current: { id: "current", bytes: 80, lastUsed: 10, successful: true, graph: original, handoff: { exact: true, complete: true } },
    retired: [{ id: "retired", bytes: 20, lastUsed: 5, successful: true }],
    usage: { complete: true, namespaceBytes: 110, freeBytes: 500 } });
  const result = planRetention(base);
  assert.equal(result.kind, "evict-retired"); assert.equal(result.unattributedBytes, 10);
  assert.equal(result.retainedBytes, 90); assert.deepEqual(result.discardIds, ["retired"]);
  assert.equal(planRetention({ ...base, current: { ...base.current, handoff: undefined } }).reason, "EXACT_PUBLICATION_PENDING");
});

test("uncertain ownership, process trees, runtime handoffs and malformed accounting fail closed", () => {
  for (const state of ["stale", "blocked", "uncertain", "missing"]) {
    const result = planRetention(observation({ lease: { state, tokenVerified: true, processTreeCertain: true } }));
    assert.equal(result.kind, "blocked"); assert.equal(result.discardIds, undefined);
  }
  for (const extra of [
    { lease: { state: "owned", tokenVerified: false, processTreeCertain: true } },
    { lease: { state: "owned", tokenVerified: true, processTreeCertain: false } },
    { candidate: candidate({ status: "uncertain" }) },
    { rotationEnabled: true, protection: { runtimeCopiesIndependent: true, pendingHandoffsSafe: false } },
    { current: { id: "../../data", bytes: 120, lastUsed: 1, successful: true, graph: original } },
    { usage: { complete: true, namespaceBytes: 1, freeBytes: 500 } },
  ]) {
    const result = planRetention(observation(extra));
    assert.equal(result.kind, "blocked"); assert.equal(result.discardIds, undefined); assert.equal(result.apply, false);
  }
  assert.equal(planRetention(observation({ usage: { complete: false, namespaceBytes: 120, freeBytes: 500 } })).reason, "MEASUREMENT_INCOMPLETE");
});

test("stdin tool emits no private inputs and cannot alter source/runtime/cache/data sentinels", async () => {
  const directory = await mkdtemp(join(tmpdir(), "rbit-retention-plan-"));
  const cli = fileURLToPath(new URL("../workspace/retention-plan.mjs", import.meta.url));
  try {
    for (const name of ["cache", "runtime", "source", "data"]) {
      await mkdir(join(directory, name)); await writeFile(join(directory, name, "sentinel"), name);
    }
    const secret = "private-profile-value";
    const input = { request: { ...request(["-pweb", "--target", "/private/target.json"]), env: { CARGO_PROFILE_TEST_DEBUG: secret } },
      observation: { ...observation(), current: { ...observation().current, request: request(["-pcore"]) } } };
    const output = execFileSync(process.execPath, [cli], { input: JSON.stringify(input), encoding: "utf8", cwd: directory, windowsHide: true });
    assert.equal(JSON.parse(output).decision.apply, false);
    assert.equal(output.includes(secret), false); assert.equal(output.includes("/private/"), false);
    assert.deepEqual((await readdir(directory)).sort(), ["cache", "data", "runtime", "source"]);
    for (const name of ["cache", "runtime", "source", "data"]) assert.equal(await readFile(join(directory, name, "sentinel"), "utf8"), name);
    const oversized = spawnSync(process.execPath, [cli], { input: "x".repeat(65537), encoding: "utf8", windowsHide: true });
    assert.notEqual(oversized.status, 0); assert.equal(oversized.stdout, "");
  } finally { await rm(directory, { recursive: true, force: true }); }
});
