import assert from "node:assert/strict";
import test from "node:test";
import { join, dirname } from "node:path";
import { link, mkdir, writeFile, readFile, copyFile, unlink, stat, utimes, open, rename } from "node:fs/promises";
import { leased } from "../workspace/lease.mjs";
import { beginSourceProof, finishSourceProof, failSourceProof } from "../workspace/source-proof/guard.mjs";
import { sourceFixture } from "./source-proof-fixture.mjs";
import { primaryProfile } from "../workspace/source-proof/units.mjs";
import { completion } from "./retention-observation-fixture.mjs";

async function command(f, body) {
  const ctx = f.context("a");
  return leased(join(ctx.cache, ".lease"), async lease => {
    const guard = await beginSourceProof(ctx, f.request(), lease, f);
    try {
      await body(guard);
      return await finishSourceProof(guard.handle, await completion(ctx, lease, { invocation: guard.invocation }), lease);
    } catch (error) { await failSourceProof(guard.handle, lease); throw error; }
  });
}
async function baseline(t, size = 1, extension = ".rmeta") {
  const f = await sourceFixture(t), ctx = f.context("a");
  let unit;
  await command(f, async guard => {
    unit = await f.unit();
    if (extension !== ".rmeta") {
      const path = unit.artifact.replace(/\.rmeta$/, extension);
      await rename(unit.artifact, path);
      unit.artifact = path;
    }
    const file = await open(unit.artifact, "r+");
    try { await file.truncate(size); } finally { await file.close(); }
    await utimes(unit.artifact, 1, 1);
    guard.observe(await f.message(ctx, unit));
  });
  const primary = join(f.lane, `target/debug/libmodel${extension}`);
  await mkdir(dirname(primary), { recursive: true });
  return { f, ctx, unit, primary };
}
async function observeExport(value, guard, mode = "link") {
  const { f, ctx, unit, primary } = value;
  if (mode === "link") await link(unit.artifact, primary);
  else await copyFile(unit.artifact, primary);
  await writeFile(join(dirname(primary), "model.d"), "derived public dependency export\n");
  const message = await f.message(ctx, unit, true);
  message.filenames.push(primary);
  guard.observe(message);
}

test("first primary-library export after dependency compilation is witnessed; subsequent warm reuse stays strict", async t => {
  const value = await baseline(t, 1, ".rlib");
  const result = await command(value.f, async guard => {
    assert.equal(guard.report.reusable, 1);
    await observeExport(value, guard);
  });
  assert.equal(result.approved, 1);
  assert.equal(result.rebuilt, 0);
  const warm = await command(value.f, async guard => guard.observe(await value.f.message(value.ctx, value.unit, true)));
  assert.equal(warm.invalidated, 0);
  assert.equal(warm.approved, 1);
});

test("same-size artifact rewrite with restored mtime cannot hide behind a new primary hardlink", async t => {
  const value = await baseline(t);
  await assert.rejects(command(value.f, async guard => {
    const before = await stat(value.unit.artifact);
    await writeFile(value.unit.artifact, "B");
    await utimes(value.unit.artifact, before.atime, before.mtime);
    assert.equal((await stat(value.unit.artifact, { bigint: true })).mtimeNs, 1000000000n);
    await observeExport(value, guard);
  }), /SOURCE_PROOF_UNEXPECTED_FRESH_UNIT/);
});

test("independent same-byte primary copy does not prove a fresh export transition", async t => {
  const value = await baseline(t);
  await assert.rejects(command(value.f, guard => observeExport(value, guard, "copy")), /SOURCE_PROOF_UNEXPECTED_FRESH_UNIT/);
});

test("missing prior companion cannot be approved by an otherwise valid export hardlink", async t => {
  const value = await baseline(t);
  await assert.rejects(command(value.f, async guard => {
    await unlink(join(dirname(value.unit.artifact), `model-${value.unit.hash}.d`));
    await observeExport(value, guard);
  }), /SOURCE_PROOF_UNEXPECTED_FRESH_UNIT/);
});

test("an unobserved alias transition is not admitted for the next command", async t => {
  const value = await baseline(t);
  const result = await command(value.f, async () => { await link(value.unit.artifact, value.primary); });
  assert.equal(result.approved, 0);
  await command(value.f, async guard => {
    assert.equal(guard.report.invalidated, 1);
    await assert.rejects(readFile(value.unit.stamp), /ENOENT/);
  });
});

test("oversized export witness invalidates only its small stamp before Cargo", async t => {
  const value = await baseline(t, 32 * 1024 ** 2 + 1);
  await command(value.f, async guard => {
    assert.equal(guard.report.invalidated, 1);
    await assert.rejects(readFile(value.unit.stamp), /ENOENT/);
    assert.equal((await stat(value.unit.artifact)).size, 32 * 1024 ** 2 + 1);
  });
});

test("an existing independent primary export requires a real rebuild instead of fresh replacement", async t => {
  const value = await baseline(t);
  // Sign the ordinary newly compiled output, including a stale independent export.
  await command(value.f, async guard => {
    await copyFile(value.unit.artifact, value.primary);
    // Force a genuine new compiler-unit control observation for this fixture.
    await writeFile(join(value.unit.directory, "invoked.timestamp"), "new compiler marker");
    guard.observe(await value.f.message(value.ctx, value.unit));
  });
  await command(value.f, async guard => {
    assert.equal(guard.report.invalidated, 1);
    await assert.rejects(readFile(value.unit.stamp), /ENOENT/);
  });
});

test("primary export mapping handles target triples in either guarded Cargo output root", () => {
  const lane = join(process.cwd(), "synthetic-lane");
  assert.equal(primaryProfile(lane, join(lane, "intermediates/triple/custom")), join(lane, "target/triple/custom"));
  assert.equal(primaryProfile(lane, join(lane, "target/triple/custom")), join(lane, "target/triple/custom"));
  assert.throws(() => primaryProfile(lane, join(lane, "other/debug")), /SOURCE_PROOF_LAYOUT_UNOBSERVED/);
});
