import assert from "node:assert/strict";
import { mkdtemp, mkdir, realpath, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { guarded } from "../workspace/paths.mjs";

async function fixture(t) {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "rbit-path-alias-")));
  t.after(() => rm(parent, { recursive: true, force: true }));
  const root = join(parent, "canonical");
  const alias = join(parent, "alias");
  const outside = join(parent, "outside");
  await mkdir(root);
  await mkdir(outside);
  const kind = process.platform === "win32" ? "junction" : "dir";
  await symlink(root, alias, kind);
  return { parent, root, alias, outside, kind };
}

test("a declared root alias admits canonical and aliased nonexisting descendants", async (t) => {
  const { root, alias } = await fixture(t);
  const destination = join(root, "future", "output");
  assert.equal(await guarded(alias, join(alias, "future", "output")), destination);
  assert.equal(await guarded(alias, destination), destination);
});

test("root alias support cannot admit siblings, root itself or linked descendants", async (t) => {
  const { root, alias, outside, kind } = await fixture(t);
  await symlink(outside, join(root, "escape"), kind);
  for (const path of [alias, root, outside, join(alias, "..", "outside", "file")]) {
    await assert.rejects(guarded(alias, path), /outside managed root/);
  }
  await assert.rejects(guarded(alias, join(alias, "escape", "file")), /Linked paths/);
  await assert.rejects(guarded(alias, join(root, "escape")), /Linked paths/);
});
