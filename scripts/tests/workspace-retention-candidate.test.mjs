import { cargo } from "./source-proof-fixture.mjs";
import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { leased, acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { failedCargoCompletion } from "../workspace/cargo-tree.mjs";
import { enforceHighWater, prune } from "../workspace/storage.mjs";
import { candidateHeadroom, requireCandidateHeadroom, createCandidate, startCandidate,
  candidateExecutionStarting, candidateExecuted, completeCandidate, quarantineCandidate } from "../workspace/retention/candidate.mjs";
import { fixture, completion, namespace } from "./retention-observation-fixture.mjs";

async function candidateRecord(candidate) {
  return JSON.parse(await readFile(join(candidate.path, "generation.json"), "utf8"));
}

test("measurement headroom is explicit, finite and checked against real observed free bytes", () => {
  for (const value of [undefined, "", "0", "-1", "01", "NaN", "9007199254740992"]) {
    assert.throws(() => candidateHeadroom({ RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: value }), /explicitly/);
  }
  assert.equal(candidateHeadroom({ RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1234" }), 1234);
  requireCandidateHeadroom(1234, 1234);
  for (const value of [null, NaN, Infinity, -1, 1233]) assert.throws(() => requireCandidateHeadroom(value, 1234), /HEADROOM_UNPROVED/);
});

test("fresh generation is separately owned and a second candidate is refused without deleting legacy output", async t => {
  const { ctx, lane } = await fixture(t);
  await writeFile(join(lane, "legacy"), "keep legacy");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const candidate = await createCandidate(ctx, namespace, 1, lease);
    assert.notEqual(candidate.path, lane);
    assert.equal(candidate.path.startsWith(join(ctx.cache, namespace, "managed")), true);
    assert.equal((await candidateRecord(candidate)).status, "allocated");
    await assert.rejects(createCandidate(ctx, namespace, 1, lease), /ALREADY_PRESENT/);
  });
  assert.equal(await readFile(join(lane, "legacy"), "utf8"), "keep legacy");
});

test("unknown and orphaned generation entries are blockers rather than deletion authority", async t => {
  for (const entry of ["candidate-orphan", "candidate.json", "unrelated"]) {
    const { ctx } = await fixture(t);
    const root = join(ctx.cache, namespace, "managed");
    await mkdir(root); await writeFile(join(root, entry), "private unrelated sentinel");
    await leased(join(ctx.cache, ".lease"), lease => assert.rejects(createCandidate(ctx, namespace, 1, lease), /ALREADY_PRESENT/));
    assert.equal(await readFile(join(root, entry), "utf8"), "private unrelated sentinel");
  }
});

test("completion publishes fresh-output provenance only after exact independent output and native proof", async t => {
  const { ctx, read, request, lane } = await fixture(t);
  await writeFile(join(lane, "legacy"), "retain");
  await leased(join(ctx.cache, ".lease"), async lease => {
    const candidate = await createCandidate(ctx, namespace, 1, lease);
    await startCandidate(candidate.handle, "build", request.args, request.env, lease, { read });
    candidateExecutionStarting(candidate.handle, lease);
    const source = join(candidate.path, "target", "app.exe");
    const destination = join(ctx.state, "bin", "published", "app.exe");
    await mkdir(join(candidate.path, "target")); await mkdir(join(ctx.state, "bin", "published"), { recursive: true });
    await writeFile(source, "exact candidate binary"); await copyFile(source, destination);
    const receipt = await completion(ctx, lease);
    candidateExecuted(candidate.handle, lease);
    await assert.rejects(completeCandidate(candidate.handle, { treeStopped: true }, [], lease), /UNPROVED/);
    const report = await completeCandidate(candidate.handle, receipt, [{ source, destination }], lease);
    assert.equal(report.status, "succeeded");
    assert.equal(report.eligibleForPromotion, false);
    assert.equal(report.measurement.automaticRotationApproved, false);
    const record = await candidateRecord(candidate);
    assert.equal(record.handoff.exact, true);
    assert.equal(record.completion.treeStopped, true);
    assert.equal(record.measurement.commandWallMs >= 0, true);
    assert.equal(record.headroom.reserved, false);
    assert.equal(JSON.stringify(record).includes(ctx.root), false);
    await assert.rejects(createCandidate(ctx, namespace, 1, lease), /ALREADY_PRESENT/);
  });
  assert.equal(await readFile(join(lane, "legacy"), "utf8"), "retain");
});

test("proved compiler failure has branded completion but remains quarantined and unpromotable", async t => {
  const { ctx, read, request } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const candidate = await createCandidate(ctx, namespace, 1, lease);
    await startCandidate(candidate.handle, request.command, request.args, request.env, lease, { read });
    candidateExecutionStarting(candidate.handle, lease);
    let failure;
    try { await completion(ctx, lease, { code: 101 }); } catch (error) { failure = error; }
    assert.ok(failedCargoCompletion(failure, lease));
    assert.equal(failedCargoCompletion({ completion: { treeStopped: true } }, lease), undefined);
    await quarantineCandidate(candidate.handle, failure, undefined, lease);
    const saved = await candidateRecord(candidate);
    assert.equal(saved.status, "failed");
    assert.equal(saved.failure.nativeTreeStopped, true);
    assert.equal(saved.eligibleForPromotion, false);
  });
});

test("uncertain failure retains the actual blocked lease and one generation without scanning it", async t => {
  const { ctx, read, request } = await fixture(t);
  let candidate;
  const pending = leased(join(ctx.cache, ".lease"), async lease => {
    candidate = await createCandidate(ctx, namespace, 1, lease);
    await startCandidate(candidate.handle, request.command, request.args, request.env, lease, { read });
    candidateExecutionStarting(candidate.handle, lease);
    try { await completion(ctx, lease, { verified: false }); }
    catch (error) { await quarantineCandidate(candidate.handle, error, undefined, lease); throw error; }
  });
  await assert.rejects(pending, UncertainProcessTreeError);
  assert.equal((await candidateRecord(candidate)).status, "uncertain");
  await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Blocked lease/);
  assert.equal((await readdir(join(ctx.cache, namespace, "managed"))).filter(name => name.startsWith("candidate-")).length, 1);
});

test("actual helper measurement targets the candidate, publishes exact output and never invokes pruning", { skip: process.platform !== "win32" }, async t => {
  const { ctx, read, lane } = await fixture(t);
  await writeFile(join(lane, "legacy"), "protected old output");
  const result = await cargo(ctx, "build", [], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "measure-candidate", RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1" },
    readRetentionInput: read,
    reportMaintenance: () => assert.fail("measurement must not prune other output"),
    execute: async (context, _args, environment, artifact, { lease }) => {
      assert.equal(environment.CARGO_TARGET_DIR.includes("candidate-"), true);
      assert.equal(environment.CARGO_BUILD_BUILD_DIR.includes("candidate-"), true);
      await mkdir(environment.CARGO_TARGET_DIR, { recursive: true });
      const executable = join(environment.CARGO_TARGET_DIR, "app.exe");
      await writeFile(executable, "candidate binary");
      artifact({ executable, target: { kind: ["bin"], name: "app" } });
      return completion(context, lease);
    },
  });
  assert.equal(result.retention.mode, "measure-candidate");
  assert.equal(result.retention.status, "succeeded");
  assert.equal(await readFile(result.executables[0].path, "utf8"), "candidate binary");
  assert.equal(await readFile(join(lane, "legacy"), "utf8"), "protected old output");
});

test("singleton reservation also bounds concurrent callers within one owned lease", async t => {
  const { ctx } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const outcomes = await Promise.allSettled([
      createCandidate(ctx, namespace, 1, lease),
      createCandidate(ctx, namespace, 1, lease),
    ]);
    assert.equal(outcomes.filter(outcome => outcome.status === "fulfilled").length, 1);
    const entries = await readdir(join(ctx.cache, namespace, "managed"));
    assert.equal(entries.filter(name => name.startsWith("candidate-")).length, 1);
  });
});

test("actual helper preserves original uncertainty when quarantine record writing also fails", { skip: process.platform !== "win32" }, async t => {
  const { ctx, read } = await fixture(t);
  const original = new UncertainProcessTreeError("original uncertainty");
  await assert.rejects(cargo(ctx, "check", [], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "measure-candidate", RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1" },
    readRetentionInput: read,
    execute: async (_context, _args, environment) => {
      // An unknown pointer refuses the diagnostic write; it must not hide the
      // original unproved process outcome or release the actual management lease.
      await writeFile(join(environment.CARGO_TARGET_DIR, "..", "..", "candidate.json"), "private unrelated record");
      throw original;
    },
  }), error => error === original && !!error.candidateRecordError);
  await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Blocked lease/);
});

test("other-namespace maintenance and manual zero prune cannot erase managed evidence", async t => {
  const { ctx } = await fixture(t);
  const other = "b".repeat(20);
  await mkdir(join(ctx.cache, other));
  await writeFile(join(ctx.cache, other, "output"), "ordinary output");
  let candidate;
  await leased(join(ctx.cache, ".lease"), async lease => {
    candidate = await createCandidate(ctx, namespace, 1, lease);
    await writeFile(join(candidate.path, "preserve"), "candidate evidence");
    const report = await enforceHighWater(ctx, 1, other, lease);
    assert.ok(report.protectedManaged.includes(namespace));
    assert.ok(report.overageBytes > 0);
    assert.equal(await readFile(join(candidate.path, "preserve"), "utf8"), "candidate evidence");
  });
  const report = await prune(ctx, 0);
  assert.ok(report.protectedManaged.includes(namespace));
  assert.equal(await readFile(join(candidate.path, "preserve"), "utf8"), "candidate evidence");
  await assert.rejects(readFile(join(ctx.cache, other, "output")), error => error.code === "ENOENT");
});

test("actual helper branded compiler failure records failed state and releases the lease", { skip: process.platform !== "win32" }, async t => {
  const { ctx, read } = await fixture(t);
  await assert.rejects(cargo(ctx, "check", [], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "measure-candidate", RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1" },
    readRetentionInput: read,
    execute: async (context, _args, _environment, _artifact, { lease }) => completion(context, lease, { code: 101 }),
  }), /exited 101/);
  const pointer = JSON.parse(await readFile(join(ctx.cache, namespace, "managed", "candidate.json"), "utf8"));
  assert.equal(pointer.status, "failed");
  assert.equal(pointer.failure.nativeTreeStopped, true);
  const release = await acquire(join(ctx.cache, ".lease"), { timeout: 0 });
  await release();
});

test("fresh output does not establish a cold compiler cache even when wrappers are visible", async t => {
  const { ctx, read, request } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const candidate = await createCandidate(ctx, namespace, 1, lease);
    await startCandidate(candidate.handle, request.command, request.args, { ...request.env, RUSTC_WRAPPER: "private wrapper path" }, lease, { read });
    candidateExecutionStarting(candidate.handle, lease);
    const receipt = await completion(ctx, lease);
    candidateExecuted(candidate.handle, lease);
    const report = await completeCandidate(candidate.handle, receipt, [], lease);
    assert.equal(report.measurement.freshOutputDirectories, true);
    assert.equal(report.measurement.wrapperEnvironmentDetected, true);
    assert.equal(report.measurement.coldCostGateEligible, false);
    assert.equal(report.measurement.compilerCacheState, "unproved");
    assert.equal(JSON.stringify(await candidateRecord(candidate)).includes("private wrapper path"), false);
  });
});

test("opaque generation handles and fresh native proof are required across state transitions", async t => {
  const { ctx, read, request } = await fixture(t);
  await leased(join(ctx.cache, ".lease"), async lease => {
    const candidate = await createCandidate(ctx, namespace, 1, lease);
    await assert.rejects(startCandidate(JSON.parse(JSON.stringify(candidate.handle)), request.command, request.args, request.env, lease, { read }), /AUTHORITY_UNPROVED/);
    assert.throws(() => candidateExecutionStarting(candidate.handle, lease), /STATE_REFUSED/);
    await startCandidate(candidate.handle, request.command, request.args, request.env, lease, { read });
    await assert.rejects(startCandidate(candidate.handle, request.command, request.args, request.env, lease, { read }), /STATE_REFUSED/);
    candidateExecutionStarting(candidate.handle, lease);
    assert.throws(() => candidateExecutionStarting(candidate.handle, lease), /STATE_REFUSED/);
    const receipt = await completion(ctx, lease);
    candidateExecuted(candidate.handle, lease);
    await assert.rejects(completeCandidate(candidate.handle, JSON.parse(JSON.stringify(receipt)), [], lease), /COMPLETION_UNPROVED/);
  });
});

test("frozen uncertainty survives quarantine and blocked-record diagnostic failures", { skip: process.platform !== "win32" }, async t => {
  const { ctx, read } = await fixture(t);
  const original = Object.freeze(new UncertainProcessTreeError("frozen original uncertainty"));
  await assert.rejects(cargo(ctx, "check", [], {
    key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "windows-job", RBIT_BUILD_MANAGED_RETENTION: "measure-candidate", RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: "1" },
    readRetentionInput: read,
    execute: async (_context, _args, environment) => {
      await writeFile(join(environment.CARGO_TARGET_DIR, "..", "..", "candidate.json"), "unknown pointer");
      await mkdir(join(ctx.cache, ".lease", "blocked.json"));
      throw original;
    },
  }), error => error === original);
  assert.equal(JSON.parse(await readFile(join(ctx.cache, ".lease", "owner.json"), "utf8")).pid, process.pid);
});
