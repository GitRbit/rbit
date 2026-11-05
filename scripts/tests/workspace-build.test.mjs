import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { cargo, compatibility } from "../workspace/build.mjs";
import { acquire } from "../workspace/lease.mjs";
import { contains, guarded } from "../workspace/paths.mjs";

const key = "0123456789abcdef0123";
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-workspace-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  return { root, checkout: root, cache: join(root, ".build-cache"), state: join(root, ".worktree-state", "test") };
}

test("containment rejects prefix siblings, root itself and linked descendants", async (t) => {
  const ctx = await fixture(t);
  assert.equal(contains(ctx.root, `${ctx.root}-outside`), false);
  assert.equal(contains(ctx.root, ctx.root), false);
  assert.equal(contains(ctx.root, join(ctx.root, "child")), true);
  const outside = await mkdtemp(join(tmpdir(), "rbit-outside-test-"));
  t.after(() => rm(outside, { recursive: true, force: true }));
  await symlink(outside, join(ctx.root, "link"), process.platform === "win32" ? "junction" : "dir");
  await assert.rejects(guarded(ctx.root, join(ctx.root, "link", "child")), /Linked paths/);
});

test("independent processes cannot enter a held lease", async (t) => {
  const ctx = await fixture(t);
  const lock = join(ctx.root, "lock");
  const release = await acquire(lock);
  const module = new URL("../workspace/lease.mjs", import.meta.url).href;
  const child = spawn(process.execPath, ["--input-type=module", "-e",
    `import { acquire } from ${JSON.stringify(module)}; const release = await acquire(process.argv[1]); console.log('entered'); await release();`, lock],
  { stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
  let output = "";
  child.stdout.on("data", (chunk) => { output += chunk; });
  const closed = new Promise((done, reject) => { child.on("close", (code) => code === 0 ? done() : reject(new Error(`child exited ${code}`))); });
  await new Promise((done) => setTimeout(done, 200));
  assert.equal(output, "");
  await release();
  await closed;
  assert.match(output, /entered/);
});

test("failed commands release the lane and never hand off stale executables", async (t) => {
  const ctx = await fixture(t);
  await assert.rejects(cargo(ctx, "build", [], { key, execute: async () => { throw new Error("compiler failure"); } }), /compiler failure/);
  const release = await acquire(join(ctx.cache, ".lease"), { timeout: 0 });
  await release();
});

test("exact Cargo artifact is copied while lease is held; both output dirs are shared", async (t) => {
  const ctx = await fixture(t);
  let observed;
  const execute = async (_ctx, args, env, artifact) => {
    observed = env;
    assert.ok(args.includes("--message-format=json-render-diagnostics"));
    await assert.rejects(acquire(join(ctx.cache, ".lease"), { timeout: 0 }), /Timed out/);
    const path = join(env.CARGO_TARGET_DIR, "debug", "rbit.exe");
    await mkdir(join(env.CARGO_TARGET_DIR, "debug"), { recursive: true });
    await writeFile(path, "correct build");
    artifact({ executable: path, target: { kind: ["bin"], name: "rbit" } });
  };
  const first = await cargo(ctx, "build", [], { key, execute, env: {} });
  const second = await cargo(ctx, "build", [], { key, execute, env: {} });
  assert.equal(await readFile(first.executables[0].path, "utf8"), "correct build");
  assert.notEqual(first.executables[0].path, second.executables[0].path);
  assert.ok(observed.CARGO_BUILD_BUILD_DIR.startsWith(ctx.cache));
  assert.ok(observed.CARGO_TARGET_DIR.startsWith(ctx.cache));
  assert.match(observed.LIBSQLITE3_FLAGS, /SQLITE_ENABLE_QPSG/);
});

test("location overrides cannot escape lanes and compatibility changes with compiler/flags", async (t) => {
  const ctx = await fixture(t);
  for (const args of [["--config=build.build-dir='elsewhere'"], ["--target-dir", "elsewhere"], ["--manifest-path=x"]]) {
    await assert.rejects(cargo(ctx, "build", args, { key }), /overrides/);
  }
  assert.notEqual(compatibility({}, "compiler A"), compatibility({}, "compiler B"));
  assert.notEqual(compatibility({}, "A"), compatibility({ RUSTFLAGS: "-C target-cpu=native" }, "A"));
});
