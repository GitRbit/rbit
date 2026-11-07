import assert from "node:assert/strict";
import test from "node:test";
import { mkdir, writeFile, access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { sessionFixture, step, plan } from "./measurement-session-fixture.mjs";
import { completion } from "./retention-observation-fixture.mjs";
import { acquire } from "../workspace/lease.mjs";

test("explicit headroom and supported common policy are preallocation gates", async t => {
  const f = await sessionFixture(t);
  await assert.rejects(f.run(plan([step()]), { env: { ...f.env,
    RBIT_BUILD_CANDIDATE_HEADROOM_BYTES: String(Number.MAX_SAFE_INTEGER) } }), /HEADROOM_UNPROVED/);
  await assert.rejects(access(join(f.ctx.cache, f.options.key, "managed")), { code: "ENOENT" });
  await mkdir(join(f.ctx.checkout, ".cargo"));
  await writeFile(join(f.ctx.checkout, ".cargo/config.toml"), 'include = "private.toml"\n');
  await assert.rejects(f.run(plan([step()])), /COMMON_POLICY_UNPROVED/);
  await assert.rejects(access(join(f.ctx.cache, f.options.key, "managed")), { code: "ENOENT" });
});

test("configuration mutation during a proved command fails source admission before copying", async t => {
  const f = await sessionFixture(t);
  await assert.rejects(f.run(plan([step("build")]), { execute: async (...args) => {
    const receipt = await f.execute(...args);
    await mkdir(join(f.ctx.checkout, ".cargo"));
    await writeFile(join(f.ctx.checkout, ".cargo/config.toml"), "[build]\njobs=1\n");
    return receipt;
  } }), /MEASUREMENT_SESSION_FAILED/);
  assert.equal((await f.saved()).status, "failed");
  await assert.rejects(access(join(f.ctx.state, "bin")), { code: "ENOENT" });
  const release = await acquire(join(f.ctx.cache, ".lease"), { timeout: 0 }); await release();
});

test("a real current-guard bin observation is independently copied only after source proof", async t => {
  const f = await sessionFixture(t), bytes = "exact synthetic executable bytes";
  const metadata = async ctx => {
    const value = await f.metadata(ctx); value.packages[0].targets[0].kind = ["bin"]; return value;
  };
  const result = await f.run(plan([step("build")]), { proofInputs: { ...f, metadata },
    execute: async (ctx, _args, env, artifact, options) => {
      const pkg = (await metadata(ctx)).packages[0], lane = dirname(env.CARGO_TARGET_DIR), hash = "b".repeat(16);
      const directory = join(lane, "intermediates/debug/.fingerprint", `model-${hash}`);
      const executable = join(lane, "intermediates/debug/deps", `model-${hash}.exe`);
      await mkdir(directory, { recursive: true }); await mkdir(dirname(executable), { recursive: true });
      await writeFile(join(directory, "bin-model"), "0123456789abcdef");
      await writeFile(join(directory, "bin-model.json"), JSON.stringify({ rustc: 1, target: 1, profile: 1, features: "[]", deps: [], local: [], rustflags: [] }));
      await writeFile(join(directory, "dep-bin-model"), "fixed depinfo");
      await writeFile(executable, bytes);
      const message = { reason: "compiler-artifact", package_id: pkg.id, target: pkg.targets[0], executable,
        filenames: [executable], features: [], fresh: false };
      options.onMessage(message); artifact(message); options.onExit(0, null);
      return completion(ctx, options.lease, { invocation: options.invocation });
    } });
  assert.equal(result.samples[0].sourceProof.approved, 1);
  assert.deepEqual(result.samples[0].handoff.files, [{ bytes: Buffer.byteLength(bytes),
    sha256: createHash("sha256").update(bytes).digest("hex") }]);
  assert.equal(result.samples[0].handoff.exact, true);
  // Current-guard ledger exists only inside this fresh temporary arm.
  assert.equal(result.samples[0].sourceProof.trusted, true);
});

test("common inherited flags and wrappers are preserved and hashed rather than silently stripped", async t => {
  const f = await sessionFixture(t);
  const env = { ...f.env, RUSTFLAGS: "--cfg synthetic_common", RUSTC_WRAPPER: "PRIVATE_WRAPPER_SENTINEL",
    CARGO_PROFILE_DEV_CODEGEN_UNITS: "4" };
  const result = await f.run(plan([step()]), { env });
  assert.equal(f.seen[0].environment.RUSTFLAGS, env.RUSTFLAGS);
  assert.equal(f.seen[0].environment.RUSTC_WRAPPER, env.RUSTC_WRAPPER);
  assert.equal(result.compilerCacheState, "unproved");
  assert.equal(result.externalCompilerOutputs, "unproved");
  assert.doesNotMatch(JSON.stringify(result), /PRIVATE_WRAPPER_SENTINEL|synthetic_common/);
});
