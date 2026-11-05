import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir, hostname } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { context, git } from "../workspace/paths.mjs";
import { create, list, remove } from "../workspace/worktrees.mjs";
import { cacheReport, prune } from "../workspace/storage.mjs";
import { acquire } from "../workspace/lease.mjs";
import { pruneGenerations } from "../workspace/runtime.mjs";
import { runtimeEnvironment } from "../workspace/ports.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-lifecycle-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  git(root, ["init", "--quiet"]);
  git(root, ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "Initial"]);
  git(root, ["update-ref", "refs/remotes/github/main", "HEAD"]);
  return context(root);
}

test("nested worktree discovers common root; dirty and active checkouts cannot be removed", async (t) => {
  const ctx = await fixture(t);
  const entry = await create(ctx, "example");
  const nested = await context(entry.path);
  assert.equal(nested.root, ctx.root);
  assert.notEqual(nested.id, ctx.id);
  assert.equal(list(ctx).length, 2);
  await writeFile(join(entry.path, "untracked"), "preserve me");
  await assert.rejects(remove(ctx, "example"), /uncommitted or untracked/);
  await rm(join(entry.path, "untracked"));
  const bin = join(nested.state, "bin", "active");
  await mkdir(bin, { recursive: true });
  await writeFile(join(bin, "running.json"), JSON.stringify({ pid: process.pid, host: hostname() }));
  await assert.rejects(remove(ctx, "example"), /active runtime/);
  await rm(join(bin, "running.json"));
  await writeFile(join(bin, "handoff.json"), JSON.stringify({ pid: process.pid, host: hostname() }));
  await assert.rejects(remove(ctx, "example"), /active runtime/);
  await rm(join(bin, "handoff.json"));
  await remove(ctx, "example");
  assert.equal(list(ctx).length, 1);
  assert.match(git(ctx.root, ["branch", "--list", "feature/example"]), /feature\/example/);
});

test("worktree names cannot traverse paths or become Git options", async (t) => {
  const ctx = await fixture(t);
  for (const name of ["../escape", "--force", "a/b", "A B"]) {
    await assert.rejects(create(ctx, name), /feature name/);
    await assert.rejects(remove(ctx, name), /feature name/);
  }
});

test("cache pruning obeys lease and LRU budget without touching source or runtime data", async (t) => {
  const ctx = await fixture(t);
  const old = join(ctx.cache, "00000000000000000000");
  const recent = join(ctx.cache, "11111111111111111111");
  for (const [path, used] of [[old, "1"], [recent, "2"]]) {
    await mkdir(path, { recursive: true });
    await writeFile(join(path, "output"), "12345");
    await writeFile(join(path, "last-used"), used);
  }
  await mkdir(ctx.state, { recursive: true });
  await writeFile(join(ctx.state, "data"), "keep");
  const release = await acquire(join(ctx.cache, ".lease"));
  await assert.rejects(prune(ctx, 0), /Timed out/);
  await release();
  const result = await prune(ctx, 6);
  assert.deepEqual(result.removed.map((entry) => entry.name), ["00000000000000000000"]);
  assert.equal((await cacheReport(ctx)).bytes, 6);
  assert.equal(await readFile(join(ctx.state, "data"), "utf8"), "keep");
});

test("cache report refuses linked descendants instead of following/deleting their targets", async (t) => {
  const ctx = await fixture(t);
  const path = join(ctx.cache, "00000000000000000000");
  await mkdir(path, { recursive: true });
  await symlink(ctx.checkout, join(path, "outside"), process.platform === "win32" ? "junction" : "dir");
  await assert.rejects(prune(ctx, 0), /linked cache entry/);
  assert.ok(git(ctx.root, ["rev-parse", "HEAD"]));
});

test("generation cleanup retains live handoffs and runtime ports/data remain independent", async (t) => {
  const ctx = await fixture(t);
  const bin = join(ctx.state, "bin", "pending");
  await mkdir(bin, { recursive: true });
  await writeFile(join(bin, "handoff.json"), JSON.stringify({ pid: process.pid, host: hostname() }));
  await pruneGenerations(ctx, 0);
  assert.match(await readFile(join(bin, "handoff.json"), "utf8"), /pid/);
  const other = { ...ctx, id: "second", state: join(ctx.root, ".worktree-state", "second") };
  const firstEnv = await runtimeEnvironment(ctx, {});
  const secondEnv = await runtimeEnvironment(other, {});
  assert.notEqual(firstEnv.RBIT_SERVER__BIND, secondEnv.RBIT_SERVER__BIND);
  assert.notEqual(firstEnv.RBIT_STORAGE__DATA_DIR, secondEnv.RBIT_STORAGE__DATA_DIR);
  assert.equal(firstEnv.RBIT_SERVER__BIND, (await runtimeEnvironment(ctx, {})).RBIT_SERVER__BIND);
});
