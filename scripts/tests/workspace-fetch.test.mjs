import assert from "node:assert/strict";
import test from "node:test";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cargo } from "../workspace/build.mjs";
import { acquire, UncertainProcessTreeError } from "../workspace/lease.mjs";
import { requireLease } from "../workspace/storage.mjs";
import { fixture, completion, namespace } from "./retention-observation-fixture.mjs";

const forbidden = () => assert.fail("Dependency fetch must never inspect or approve source provenance");
const provenance = { begin: forbidden, finish: forbidden, fail: forbidden };

test("cold dependency fetch owns the lease and both Cargo directories without reading metadata or admitting units", async t => {
  const { ctx, lane } = await fixture(t);
  const stamps = join(lane, "intermediates/debug/.fingerprint/model-aaaaaaaaaaaaaaaa");
  await mkdir(stamps, { recursive: true });
  const stamp = join(stamps, "lib-model");
  await writeFile(stamp, "preserve existing freshness bytes");
  const ledger = join(lane, ".source-proof.json");
  await writeFile(ledger, "not a trusted ledger and fetch must not read it");
  let receipt;
  const result = await cargo(ctx, "fetch", ["--locked"], { key: namespace,
    env: { RBIT_WORKSPACE_CARGO_TREE: "off", LIBSQLITE3_FLAGS: "-DEXISTING" }, provenance,
    proofInputs: { metadata: forbidden },
    execute: async (actual, args, env, artifact, options) => {
      assert.equal(actual, ctx);
      assert.deepEqual(args, ["fetch", "--locked"]);
      await requireLease(ctx, options.lease);
      await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /lease/i);
      assert.equal(env.CARGO_TARGET_DIR, join(lane, "target"));
      assert.equal(env.CARGO_BUILD_TARGET_DIR, env.CARGO_TARGET_DIR);
      assert.equal(env.CARGO_BUILD_BUILD_DIR, join(lane, "intermediates"));
      assert.match(env.LIBSQLITE3_FLAGS, /-DEXISTING.*-DSQLITE_ENABLE_QPSG/);
      assert.equal(options.invocation, undefined);
      assert.equal(options.onMessage, undefined);
      receipt = await completion(ctx, options.lease);
      return receipt;
    },
  });
  assert.equal(result.completion, receipt);
  assert.equal(result.sourceProof, undefined);
  assert.deepEqual(result.executables, []);
  assert.equal(await readFile(stamp, "utf8"), "preserve existing freshness bytes");
  assert.equal(await readFile(ledger, "utf8"), "not a trusted ledger and fetch must not read it");
  await assert.rejects(readFile(join(lane, ".source-proof.key")), /ENOENT/);
  await assert.rejects(readFile(join(ctx.state, "bin")), /ENOENT/);
  const lease = await acquire(join(ctx.cache, ".lease"), { timeout: 0 });
  await lease();
});

test("fetch rejects managed-retention modes and output escapes before execution", async t => {
  const { ctx } = await fixture(t);
  const execute = () => assert.fail("Rejected fetch must not execute");
  for (const mode of ["observe", "measure-candidate"]) {
    await assert.rejects(cargo(ctx, "fetch", ["--locked"], { key: namespace, execute,
      env: { RBIT_WORKSPACE_CARGO_TREE: "off", RBIT_BUILD_MANAGED_RETENTION: mode }, provenance,
    }), /fetch does not support managed retention/);
  }
  for (const args of [["--target-dir", "elsewhere"], ["--build-dir=elsewhere"], ["--config", "build.target-dir='elsewhere'"]]) {
    await assert.rejects(cargo(ctx, "fetch", args, { key: namespace, env: {}, execute, provenance }), /overrides/);
  }
});

test("unproved fetch process failure retains the blocked writer lease", async t => {
  const { ctx } = await fixture(t);
  await assert.rejects(cargo(ctx, "fetch", ["--locked"], { key: namespace, env: { RBIT_WORKSPACE_CARGO_TREE: "off" }, provenance,
    execute: async () => { throw new UncertainProcessTreeError("Synthetic fetch descendants unproved"); },
  }), UncertainProcessTreeError);
  await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Blocked lease/);
});

test("every hosted helper compiler job bootstraps locked dependencies before offline admission", async () => {
  const workflows = new URL("../../.github/workflows/", import.meta.url);
  let guardedJobs = 0;
  for (const name of await readdir(workflows)) {
    if (!/\.ya?ml$/.test(name)) continue;
    const text = (await readFile(new URL(name, workflows), "utf8")).replaceAll("\r\n", "\n");
    for (const block of text.split(/^  [a-z][a-z0-9-]*:\n/m).slice(1)) {
      const first = block.search(/node (?:scripts\/workspace\.mjs (?:test|check|build|clippy|nextest|ide-check|run)\b|scripts\/rewrite\/check-types\.mjs)/);
      if (first < 0) continue;
      guardedJobs++;
      const fetch = block.indexOf("node scripts/workspace.mjs fetch --locked");
      assert.ok(fetch >= 0 && fetch < first, `${name}: bootstrap before first guarded compiler`);
    }
  }
  assert.equal(guardedJobs, 2, "contracts matrix and native ownership experiment");
  const cli = await readFile(new URL("../workspace.mjs", import.meta.url), "utf8");
  assert.match(cli, /"generate-lockfile", "fetch"\]\.includes\(command\)/);
});
