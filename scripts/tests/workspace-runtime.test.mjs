import assert from "node:assert/strict";
import { access, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { hostname, tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import test from "node:test";
import { alive, leased } from "../workspace/lease.mjs";
import { launchTree, stopTree, verifyTreeStopped } from "../workspace/process-tree.mjs";
import { finishRuntime, protectedRuntime } from "../workspace/runtime-marker.mjs";
import { observeOwnedLaunch, preservingLaunchFailure } from "./support/owned-launch-observation.mjs";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

test("response-file transport preserves native spawn rejection of non-string or NUL arguments", () => {
  for (const args of [["nul\0argument"], [null], "not-an-array"]) {
    assert.throws(() => launchTree(process.execPath, args, {}), /non-NUL strings/);
  }
});
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "rbit-runtime-test-"));
  const ctx = { root, checkout: root, cache: join(root, ".build-cache"), state: join(root, "state") };
  t.after(async () => {
    // A cancelled native test must finish owned cleanup before removing stop/proof files.
    try { await ctx.cleanup?.(); }
    catch (error) {
      if (!ctx.primaryFailure && !t.signal.aborted) throw error;
      t.diagnostic("OWNED_LAUNCH_CLEANUP_UNPROVED");
      return; // Keep control evidence; the original assertion/deadline remains the failure.
    }
    await rm(root, { recursive: true, force: true });
  });
  const cache = join(root, ".build-cache"); await mkdir(cache);
  return ctx;
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
  const expected = ["with spaces", "embedded\"quote", "trailing\\", "", "a\\\"b", "a".repeat(26_000)];
  const source = "const fs=require('node:fs');fs.writeFileSync('target-stage','started');" +
    "console.log(JSON.stringify({args:process.argv.slice(1),cwd:process.cwd(),env:process.env.WORKSPACE_ARG_TEST}));" +
    "fs.writeFileSync('target-stage','output-returned');process.exit(7)";
  const { child } = launch(ctx, ["-e", source, ...expected]);
  const observation = observeOwnedLaunch(child, { progress: async () => {
    const read = async (file) => {
      try { return await readFile(file, "utf8"); }
      catch (error) { if (error.code === "ENOENT") return null; throw error; }
    };
    const stage = await read(join(ctx.root, "target-stage"));
    const proof = await read(child.treeControl.proofFile);
    let argumentsPending = true;
    try { await access(`${child.treeControl.stopFile}.args.json`); }
    catch (error) { if (error.code === "ENOENT") argumentsPending = false; else throw error; }
    return {
      argumentsPending,
      targetStarted: stage === "started" || stage === "output-returned",
      targetOutputReturned: stage === "output-returned",
      proof: proof === null ? "missing" : proof === child.treeControl.token ? "verified" : "unreadable",
    };
  } });
  ctx.cleanup = () => observation.cleanup();
  // Report before node:test's unchanged 30s deadline, without changing success criteria.
  const watchdog = setTimeout(() => {
    observation.diagnostics().then((state) => t.diagnostic(JSON.stringify(state)));
  }, 25_000);
  try {
    await preservingLaunchFailure(async () => {
      assert.equal((await observation.wait(t.signal)).code, 7, "OWNED_LAUNCH_EXIT_CODE");
      let parsed;
      try { parsed = JSON.parse(observation.output()); }
      catch { assert.fail("OWNED_LAUNCH_INVALID_JSON"); }
      assert.equal(isDeepStrictEqual(parsed.args, expected), true, "OWNED_LAUNCH_ARGUMENTS");
      assert.equal(parsed.env === "kept", true, "OWNED_LAUNCH_ENVIRONMENT");
      assert.equal(parsed.cwd === ctx.root, true, "OWNED_LAUNCH_CWD");
      assert.equal(await verifyTreeStopped(child), true, "OWNED_LAUNCH_PROOF");
    }, ctx.cleanup);
  } catch (error) {
    ctx.primaryFailure = true;
    t.diagnostic(JSON.stringify(await observation.diagnostics()));
    throw error;
  } finally { clearTimeout(watchdog); }
});

test("failed target spawn acknowledges that no owned target survived", { timeout: 30_000 }, async (t) => {
  const ctx = await fixture(t);
  const { child, closed } = launch(ctx, [], join(ctx.root, "missing target.exe"));
  child.on("error", () => {}); child.stderr.resume();
  const result = await closed;
  assert.notEqual(result.code, 0);
  assert.equal(await verifyTreeStopped(child), true);
});

test("Windows normal exit 259 is preserved after the target wait signals", { timeout: 30_000, skip: process.platform !== "win32" }, async (t) => {
  const ctx = await fixture(t);
  const { child } = launch(ctx, ["-e", "process.exit(259)"]);
  const observation = observeOwnedLaunch(child);
  ctx.cleanup = () => observation.cleanup();
  await preservingLaunchFailure(async () => {
    assert.equal((await observation.wait(t.signal)).code, 259, "OWNED_LAUNCH_NORMAL_EXIT_259");
    assert.equal(await verifyTreeStopped(child), true, "OWNED_LAUNCH_EXIT_259_PROOF");
  }, ctx.cleanup);
});

test("Windows stop of a still-running target remains a successful owned shutdown", { timeout: 30_000, skip: process.platform !== "win32" }, async (t) => {
  const ctx = await fixture(t);
  const source = "require('node:fs').writeFileSync('running.json',JSON.stringify(process.pid));setInterval(()=>{},1000)";
  const { child } = launch(ctx, ["-e", source]);
  const observation = observeOwnedLaunch(child);
  ctx.cleanup = () => observation.cleanup();
  await preservingLaunchFailure(async () => {
    const pid = await waitFor(join(ctx.root, "running.json")).catch(() => assert.fail("OWNED_LAUNCH_RUNNING_READY"));
    assert.equal(alive(pid), true, "OWNED_LAUNCH_TARGET_RUNNING");
    await stopTree(child);
    assert.equal((await observation.wait(t.signal)).code, 0, "OWNED_LAUNCH_STOP_EXIT");
    assert.equal(await verifyTreeStopped(child), true, "OWNED_LAUNCH_STOP_PROOF");
    assert.equal(alive(pid), false, "OWNED_LAUNCH_TARGET_STOPPED");
  }, ctx.cleanup);
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

test("abrupt launcher death before first target instruction cannot strand an unowned suspended process", { timeout: 30_000, skip: process.platform !== "win32" }, async (t) => {
  const ctx = await fixture(t);
  const token = randomUUID();
  const ready = join(ctx.root, "suspended.json");
  const child = launchTree(process.execPath, ["-e", "setInterval(()=>{},1000)"], {
    cwd: ctx.root, env: process.env, stdio: ["ignore", "pipe", "pipe"],
    token, stopFile: join(ctx.root, "stop"), proofFile: join(ctx.root, "proof"),
    suspendedReady: ready, resumeGate: join(ctx.root, "never-resume"),
  });
  child.stderr.resume();
  const closed = new Promise((done) => child.once("close", done));
  const suspendedPid = await waitFor(ready);
  assert.equal(alive(suspendedPid), true);
  child.kill("SIGKILL"); await closed;
  for (let attempt = 0; attempt < 100 && alive(suspendedPid); attempt++) await pause(100);
  assert.equal(alive(suspendedPid), false);
  assert.equal(await verifyTreeStopped(child), false);
});

test("Unix stop escalates an owned SIGTERM-ignoring group before waiting for pipe close", { timeout: 30_000, skip: process.platform === "win32" }, async (t) => {
  const ctx = await fixture(t);
  const file = join(ctx.root, "ignore.mjs");
  await writeFile(file, `import {spawn} from 'node:child_process';import {writeFileSync} from 'node:fs';
const child=spawn(process.execPath,['-e',"process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"],{stdio:'inherit'});
process.on('SIGTERM',()=>{});setInterval(()=>{},1000);writeFileSync('ignoring.json',JSON.stringify({root:process.pid,child:child.pid}));`);
  const { child, closed } = launch(ctx, [file]);
  child.stderr.resume();
  const pids = await waitFor(join(ctx.root, "ignoring.json"));
  await pause(200);
  await stopTree(child); await closed;
  assert.equal(alive(pids.root), false); assert.equal(alive(pids.child), false);
});
