import assert from "node:assert/strict";
import test from "node:test";
import { readFile, writeFile, access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { sessionFixture, step, plan } from "./measurement-session-fixture.mjs";

test("bounded profile matrix uses current real source proof for fresh and warm units under one lease", async t => {
  const f = await sessionFixture(t);
  await writeFile(join(f.lane, "legacy"), "protected legacy bytes");
  const steps = [step("build"), step(), step("clippy", ["-p", "model", "--", "-D", "warnings"]),
    step("test", ["-p", "model", "--", "--nocapture"]), step("nextest", ["run", "-p", "model"]), step("build")];
  const result = await f.run({ schema: 1, arms: [{ steps }, { steps, environment: { CARGO_PROFILE_DEV_DEBUG: "0" } }] });
  assert.equal(result.status, "succeeded");
  assert.equal(result.samples.length, 12);
  assert.deepEqual(result.samples.map(value => value.arm), [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0]);
  assert.ok(result.samples.every(value => value.sourceProof.trusted && value.sourceProof.approved === 1));
  assert.equal(result.samples.filter(value => value.sourceProof.rebuilt === 1).length, 2);
  assert.equal(result.samples.filter(value => value.sourceProof.fresh === 1).length, 10);
  assert.equal(new Set(f.seen.map(value => value.environment.CARGO_TARGET_DIR)).size, 2);
  for (const { environment } of f.seen) {
    assert.equal(environment.CARGO_TARGET_DIR, environment.CARGO_BUILD_TARGET_DIR);
    assert.equal(dirname(environment.CARGO_TARGET_DIR), dirname(environment.CARGO_BUILD_BUILD_DIR));
    assert.notEqual(environment.CARGO_TARGET_DIR, environment.CARGO_BUILD_BUILD_DIR);
    assert.ok(environment.LIBSQLITE3_FLAGS.includes("-DSQLITE_ENABLE_QPSG"));
  }
  assert.equal(await readFile(join(f.lane, "legacy"), "utf8"), "protected legacy bytes");
  assert.equal(result.coldCostGateEligible, false);
  assert.equal(result.eligibleForPromotion, false);
  assert.doesNotMatch(JSON.stringify(result), /PRIVATE|CARGO_HOME|rustflags|worktrees|\.build-cache/);
  await assert.rejects(f.run(plan([step()])), /ALREADY_PRESENT/);
  const release = await acquire(join(f.ctx.cache, ".lease"), { timeout: 0 }); await release();
});

test("a stale same-lease native receipt cannot authorize the next command", async t => {
  const f = await sessionFixture(t); let prior;
  await assert.rejects(f.run(plan([step(), step()]), { execute: async (...args) => {
    const receipt = await f.execute(...args);
    if (!prior) { prior = receipt; return receipt; }
    return prior;
  } }), UncertainProcessTreeError);
  assert.equal((await f.saved()).status, "uncertain");
  await access(join(f.ctx.cache, ".lease/blocked.json"));
});

test("current guard rejects source ABA before any executable handoff", async t => {
  const f = await sessionFixture(t);
  await assert.rejects(f.run(plan([step("build")]), { execute: async (...args) => {
    const path = join(f.ctx.checkout, "tools/model/src/lib.rs");
    const bytes = await readFile(path);
    await writeFile(path, "pub const VALUE: &str = \"B\";\n");
    const receipt = await f.execute(...args);
    await writeFile(path, bytes);
    return receipt;
  } }), /MEASUREMENT_SESSION_FAILED/);
  const record = await f.saved();
  assert.equal(record.status, "failed");
  assert.equal(record.failedSample.treeStopped, true);
  await assert.rejects(access(join(f.ctx.state, "bin")), { code: "ENOENT" });
  const release = await acquire(join(f.ctx.cache, ".lease"), { timeout: 0 }); await release();
});

test("unknown execution plus failed ledger publication still quarantines the global lease", async t => {
  const f = await sessionFixture(t);
  const provenance = { begin: async () => ({ handle: {}, invocation: {}, observe: () => {} }),
    finish: () => assert.fail("no completion"), fail: async () => { throw Error("PRIVATE_LEDGER_ERROR"); } };
  await assert.rejects(f.run(plan([step()]), { provenance, execute: async () => { throw Error("PRIVATE_CHILD_ERROR"); } }), UncertainProcessTreeError);
  assert.equal((await f.saved()).status, "uncertain");
  assert.doesNotMatch(JSON.stringify(await f.saved()), /PRIVATE/);
  await access(join(f.ctx.cache, ".lease/blocked.json"));
});

test("native compiler failure retains its actual exit and leaves the successful legacy lane intact", async t => {
  const f = await sessionFixture(t);
  await assert.rejects(f.run(plan([step()]), { execute: async (ctx, _args, _env, _artifact, options) => {
    options.onExit(101, null);
    return completion(ctx, options.lease, { code: 101, invocation: options.invocation });
  } }), /MEASUREMENT_SESSION_FAILED/);
  const record = await f.saved();
  assert.equal(record.status, "failed");
  assert.deepEqual(record.failedSample.exit, { code: 101, signal: null });
  assert.equal(record.failedSample.treeStopped, true);
  const release = await acquire(join(f.ctx.cache, ".lease"), { timeout: 0 }); await release();
});
