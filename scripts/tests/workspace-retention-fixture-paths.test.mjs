import assert from "node:assert/strict";
import test from "node:test";
import { copyFile, mkdtemp, mkdir, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { proveHandoff } from "../workspace/retention/handoff.mjs";
import { fixture, namespace } from "./retention-observation-fixture.mjs";

async function assertCanonicalHandoff(ctx, lane) {
  assert.equal(ctx.root, await realpath(ctx.root));
  assert.equal(ctx.checkout, ctx.root);
  assert.equal(ctx.cache, join(ctx.root, "cache"));
  assert.equal(ctx.state, join(ctx.root, "state"));
  assert.equal(lane, join(ctx.cache, namespace, "shared"));
  const source = join(lane, "app.exe");
  const destination = join(ctx.state, "bin", "generation", "app.exe");
  await mkdir(join(ctx.state, "bin", "generation"), { recursive: true });
  await writeFile(source, "exact fixture output");
  await copyFile(source, destination);
  const proof = await proveHandoff(ctx, lane, [{ source, destination }]);
  assert.equal(proof.exact, true);
  assert.equal(proof.files.length, 1);
}

test("temporary retention contexts derive all paths from the canonical root", async t => {
  const { ctx, lane } = await fixture(t);
  await assertCanonicalHandoff(ctx, lane);
});

test("an aliased TEMP parent still creates canonical contexts and independent handoffs", async t => {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "rbit-retention-temp-alias-")));
  t.after(() => rm(parent, { recursive: true, force: true }));
  const canonicalParent = join(parent, "Canonical Long Temporary Parent");
  const alias = join(parent, "TEMPAL~1");
  await mkdir(canonicalParent);
  // A junction exercises the same alias-to-canonical path distinction as hosted
  // Windows 8.3 TEMP without requiring 8.3-name creation to be enabled locally.
  await symlink(canonicalParent, alias, process.platform === "win32" ? "junction" : "dir");
  const { ctx, lane } = await fixture(t, { temporaryRoot: alias });
  assert.equal(ctx.root.startsWith(canonicalParent), true);
  assert.equal(ctx.root.startsWith(alias), false);
  await assertCanonicalHandoff(ctx, lane);
});
