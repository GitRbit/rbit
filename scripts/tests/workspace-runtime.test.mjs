import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { hostname, tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import test from "node:test";
import { alive, leased } from "../workspace/lease.mjs";
import { launchTree, stopTree, verifyTreeStopped } from "../workspace/process-tree.mjs";
import { finishRuntime, protectedRuntime } from "../workspace/runtime-marker.mjs";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-runtime-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const cache = join(root, ".build-cache"); await mkdir(cache);
  return { root, checkout: root, cache, state: join(root, "state") };
}

test("delayed old cleanup cannot unlink a new runtime owner's marker", async (t) => {
  const ctx = await fixture(t);
  const marker = join(ctx.root, "running.json");
  await writeFile(marker, JSON.stringify({ token: "old", pid: process.pid, host: hostname() }));
  let entered, release;
  const inRemoval = new Promise((done) => { entered = done; });
  const gate = new Promise((done) => { release = done; });
  const old = finishRuntime(ctx, marker, "old", true, { remove: async (...args) => { entered(); await gate; await rm(...args); } });
  await inRemoval;
  let published = false;
  const next = leased(join(ctx.cache, ".lease"), async () => {
    await writeFile(marker, JSON.stringify({ token: "new", pid: process.pid, host: hostname() })); published = true;
  });
  await pause(150); assert.equal(published, false);
  release(); await old; await next;
  await finishRuntime(ctx, marker, "old", true);
  assert.equal(JSON.parse(await readFile(marker, "utf8")).token, "new");
});

test("unverified descendants retain ownership even after the supervisor PID is dead", async (t) => {
  const ctx = await fixture(t);
  const marker = join(ctx.root, "running.json");
  await writeFile(marker, JSON.stringify({ token: "mine", pid: -1, host: hostname() }));
  await finishRuntime(ctx, marker, "mine", false);
  const record = JSON.parse(await readFile(marker, "utf8"));
  assert.equal(record.uncertain, true);
  assert.equal(protectedRuntime(record), true);
  assert.equal(protectedRuntime({ pid: -1, host: hostname(), requiresProof: true }), true);
});

function launch(ctx, args, executable = process.execPath) {
  const token = randomUUID();
  const child = launchTree(executable, args, {
    cwd: ctx.root, env: { ...process.env, WORKSPACE_ARG_TEST: "kept" }, stdio: ["ignore", "pipe", "pipe"],
    stopFile: join(ctx.root, `${token}.stop`), proofFile: join(ctx.root, `${token}.proof`), token,
  });
  const closed = new Promise((done) => child.once("close", (code, signal) => done({ code, signal })));
  return { child, closed };
}

async function waitFor(file, predicate = () => true) {
  for (let attempt = 0; attempt < 200; attempt++) {
    try { const value = JSON.parse(await readFile(file, "utf8")); if (predicate(value)) return value; }
    catch (error) { if (error.code !== "ENOENT" && !(error instanceof SyntaxError)) throw error; }
    await pause(100);
  }
  throw new Error(`Timed out waiting for test process state ${file}`);
}

test("owned launcher preserves quoting, cwd, environment, output and normal exit code", { timeout: 30_000 }, async (t) => {
  const ctx = await fixture(t);
  const expected = ["with spaces", "embedded\"quote", "trailing\\", "", "a\\\"b"];
  const { child, closed } = launch(ctx, ["-e", "console.log(JSON.stringify({args:process.argv.slice(1),cwd:process.cwd(),env:process.env.WORKSPACE_ARG_TEST}));process.exit(7)", ...expected]);
  let output = "", errors = "";
  child.stdout.on("data", (chunk) => { output += chunk; }); child.stderr.on("data", (chunk) => { errors += chunk; });
  assert.equal((await closed).code, 7, errors);
  assert.deepEqual(JSON.parse(output).args, expected);
  assert.equal(JSON.parse(output).env, "kept");
  assert.equal(JSON.parse(output).cwd, ctx.root);
  assert.equal(await verifyTreeStopped(child), true);
});

test("failed target spawn acknowledges that no owned target survived", { timeout: 30_000 }, async (t) => {
  const ctx = await fixture(t);
  const { child, closed } = launch(ctx, [], join(ctx.root, "missing target.exe"));
  child.on("error", () => {}); child.stderr.resume();
  const result = await closed;
  assert.notEqual(result.code, 0);
  assert.equal(await verifyTreeStopped(child), true);
});

async function supervisor(ctx) {
  const file = join(ctx.root, "supervisor.mjs");
  await writeFile(file, `import {spawn} from 'node:child_process';import {writeFileSync,existsSync,unlinkSync} from 'node:fs';
let generation=0,child;const start=()=>{child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});writeFileSync('pids.json',JSON.stringify({root:process.pid,child:child.pid,generation:generation++}));};start();
setInterval(()=>{if(existsSync('restart')){unlinkSync('restart');child.once('exit',start);child.kill();}},50);
process.on('SIGTERM',()=>{child.once('exit',()=>process.exit(0));child.kill();});`);
  return launch(ctx, [file]);
}

test("owned supervisor restart and stop leave no worker descendants", { timeout: 40_000 }, async (t) => {
  const ctx = await fixture(t);
  const { child, closed } = await supervisor(ctx);
  let errors = ""; child.stderr.on("data", (chunk) => { errors += chunk; });
  t.after(async () => { if (child.exitCode === null && child.signalCode === null) await stopTree(child); });
  const first = await waitFor(join(ctx.root, "pids.json"));
  await writeFile(join(ctx.root, "restart"), "restart");
  const second = await waitFor(join(ctx.root, "pids.json"), (value) => value.generation === 1);
  assert.notEqual(second.child, first.child); assert.equal(alive(first.child), false);
  await stopTree(child); await closed;
  assert.equal(await verifyTreeStopped(child), true, errors);
  assert.equal(alive(second.root), false); assert.equal(alive(second.child), false);
});

test("abrupt Windows launcher death kills its job but lacks a cleanup acknowledgement", { timeout: 40_000, skip: process.platform !== "win32" }, async (t) => {
  const ctx = await fixture(t);
  const { child, closed } = await supervisor(ctx);
  child.stderr.resume();
  const pids = await waitFor(join(ctx.root, "pids.json"));
  child.kill("SIGKILL"); await closed;
  const verified = await verifyTreeStopped(child);
  assert.equal(verified, process.platform !== "win32");
  for (let attempt = 0; attempt < 100 && (alive(pids.root) || alive(pids.child)); attempt++) await pause(100);
  assert.equal(alive(pids.root), false); assert.equal(alive(pids.child), false);
});
