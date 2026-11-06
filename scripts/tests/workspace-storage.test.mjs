import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { leased } from "../workspace/lease.mjs";
import { cacheBudget, cacheReport, DEFAULT_CACHE_MAX_BYTES, enforceHighWater, prune } from "../workspace/storage.mjs";

const old = "00000000000000000000", recent = "11111111111111111111", current = "22222222222222222222";
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-soft-cache-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const ctx = { root, checkout: root, cache: join(root, ".build-cache"), state: join(root, ".worktree-state") };
  await mkdir(ctx.cache); await mkdir(ctx.state);
  await writeFile(join(ctx.state, "data"), "keep runtime data");
  await writeFile(join(root, "source.rs"), "keep source");
  return ctx;
}

async function namespace(ctx, name, used) {
  const path = join(ctx.cache, name); await mkdir(path, { recursive: true });
  await writeFile(join(path, "artifact"), "12345");
  await writeFile(join(path, "last-used"), String(used));
}

test("automatic budget defaults to 16 GiB, accepts zero disable and rejects invalid byte counts", () => {
  assert.equal(cacheBudget({}), DEFAULT_CACHE_MAX_BYTES);
  assert.equal(DEFAULT_CACHE_MAX_BYTES, 17_179_869_184);
  assert.equal(cacheBudget({ RBIT_BUILD_CACHE_MAX_BYTES: "0" }), 0);
  assert.equal(cacheBudget({ RBIT_BUILD_CACHE_MAX_BYTES: "42" }), 42);
  for (const value of ["", " ", "-1", "1.5", "Infinity", "16GiB", "9007199254740992", 0]) {
    assert.throws(() => cacheBudget({ RBIT_BUILD_CACHE_MAX_BYTES: value }), /nonnegative safe integer/);
  }
});

test("automatic LRU evicts only inactive namespaces and preserves current, source and runtime", async (t) => {
  const ctx = await fixture(t);
  await namespace(ctx, old, 1); await namespace(ctx, recent, 2); await namespace(ctx, current, 0);
  const result = await leased(join(ctx.cache, ".lease"), (lease) => enforceHighWater(ctx, 12, current, lease));
  assert.deepEqual(result.removed.map((entry) => entry.name), [old]);
  assert.equal(result.bytes, 12); assert.equal(result.overageBytes, 0);
  assert.equal(await readFile(join(ctx.cache, current, "artifact"), "utf8"), "12345");
  assert.equal(await readFile(join(ctx.state, "data"), "utf8"), "keep runtime data");
  assert.equal(await readFile(join(ctx.root, "source.rs"), "utf8"), "keep source");
});

test("protected namespace can exceed soft water without deletion or false bound claims", async (t) => {
  const ctx = await fixture(t);
  await namespace(ctx, old, 1); await namespace(ctx, current, 0);
  const result = await leased(join(ctx.cache, ".lease"), (lease) => enforceHighWater(ctx, 2, current, lease));
  assert.equal(result.bytes, 6); assert.equal(result.overageBytes, 4);
  assert.deepEqual(result.removed.map((entry) => entry.name), [old]);
  assert.equal((await cacheReport(ctx)).bytes, 6);
});

test("zero disables automatic pruning but leaves manual zero-budget pruning intact", async (t) => {
  const ctx = await fixture(t);
  await namespace(ctx, old, 1); await namespace(ctx, current, 0);
  const result = await leased(join(ctx.cache, ".lease"), (lease) => enforceHighWater(ctx, 0, current, lease));
  assert.equal(result.disabled, true); assert.equal((await cacheReport(ctx)).entries.length, 2);
  const manual = await prune(ctx, 0);
  assert.equal(manual.bytes, 0); assert.equal(manual.removed.length, 2);
});

test("automatic maintenance requires live lease ownership and rejects stale nonces", async (t) => {
  const ctx = await fixture(t);
  await namespace(ctx, current, 0);
  await assert.rejects(enforceHighWater(ctx, 2, current), /current management lease/);
  let stale;
  await leased(join(ctx.cache, ".lease"), (lease) => { stale = lease; });
  await leased(join(ctx.cache, ".lease"), async (lease) => {
    await assert.rejects(enforceHighWater(ctx, 2, current, stale), /current management lease/);
    const report = await enforceHighWater(ctx, 2, current, lease);
    assert.equal(report.overageBytes, 4);
  });
});
