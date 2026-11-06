import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { observeOwnedLaunch, preservingLaunchFailure } from "./support/owned-launch-observation.mjs";

function childFixture() {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.exitCode = null;
  child.signalCode = null;
  child.kills = 0;
  child.kill = () => { child.kills++; child.signalCode = "SIGKILL"; return true; };
  return child;
}

function timers() {
  const pending = new Map();
  let next = 0;
  return {
    schedule(callback) { pending.set(++next, callback); return next; },
    cancel(id) { pending.delete(id); },
    fire() {
      assert.equal(pending.size, 1, "exactly one total cleanup deadline");
      [...pending.values()][0]();
    },
    get size() { return pending.size; },
  };
}

test("deadline cancellation runs one owned stop and retains the original failure", async () => {
  const child = childFixture();
  const clock = timers();
  let stops = 0;
  const observation = observeOwnedLaunch(child, {
    ...clock,
    stop: async () => { stops++; child.emit("exit", 0, null); child.emit("close", 0, null); },
    verify: async () => true,
  });
  const controller = new AbortController();
  const run = preservingLaunchFailure(() => observation.wait(controller.signal), () => observation.cleanup());
  controller.abort();
  await assert.rejects(run, { code: "OWNED_LAUNCH_DEADLINE" });
  await observation.cleanup();
  assert.equal(stops, 1);
  assert.equal(child.kills, 0);
  assert.equal((await observation.diagnostics()).cleanupPhase, "verified");
  assert.equal(clock.size, 0);
});

test("root exit with an open pipe remains pending and bounded cleanup cannot claim proof", async () => {
  const child = childFixture();
  const clock = timers();
  let verifications = 0;
  const observation = observeOwnedLaunch(child, {
    ...clock, stop: async () => {}, verify: async () => { verifications++; return true; },
  });
  const controller = new AbortController();
  const waited = observation.wait(controller.signal);
  child.exitCode = 7;
  child.emit("exit", 7, null);
  const state = await observation.diagnostics();
  assert.equal(state.exited, true);
  assert.equal(state.closed, false);
  controller.abort();
  await assert.rejects(waited, { code: "OWNED_LAUNCH_DEADLINE" });
  const cleanup = observation.cleanup();
  clock.fire();
  await assert.rejects(cleanup, { code: "OWNED_LAUNCH_CLEANUP_UNPROVED" });
  assert.equal(verifications, 0);
  assert.equal(child.kills, 0, "do not signal a PID after its owned root already exited");
  assert.equal(child.stdout.destroyed, true);
  assert.equal(child.stderr.destroyed, true);
  assert.equal(clock.size, 0);
  child.emit("close", 7, null);
  await Promise.resolve();
  await Promise.resolve();
  assert.equal((await observation.diagnostics()).cleanupPhase, "unproved", "late pipe closure cannot turn uncertainty into success");
  await assert.rejects(observation.cleanup(), { code: "OWNED_LAUNCH_CLEANUP_UNPROVED" });
});

test("spawn errors and private stderr become fixed bounded diagnostics", async () => {
  const child = childFixture();
  const observation = observeOwnedLaunch(child, {
    stop: async () => { child.emit("close", 125, null); }, verify: async () => true,
    progress: async () => ({ proof: "verified", targetStarted: true, path: "PRIVATE_PATH", args: "PRIVATE_ARG", env: "PRIVATE_ENV" }),
  });
  const waited = observation.wait();
  child.stderr.write("PRIVATE_STDERR");
  child.emit("error", new Error("PRIVATE_SPAWN_ERROR"));
  await assert.rejects(waited, { code: "OWNED_LAUNCH_SPAWN_ERROR" });
  await observation.cleanup();
  const state = await observation.diagnostics();
  assert.equal(state.stderrBytes, Buffer.byteLength("PRIVATE_STDERR"));
  assert.equal(state.targetStarted, true);
  assert.equal(JSON.stringify(state).includes("PRIVATE"), false);
});

test("a failed operation keeps precedence over unproved owned cleanup", async () => {
  const primary = new Error("OWNED_LAUNCH_ARGUMENTS");
  await assert.rejects(preservingLaunchFailure(
    async () => { throw primary; }, async () => { throw new Error("PRIVATE_CLEANUP_ERROR"); },
  ), (error) => error === primary && error.cleanupUnproved === true && error.cause === undefined);
});

test("cleanup failure after a successful operation fails rather than downgrading to pass", async () => {
  const child = childFixture();
  const observation = observeOwnedLaunch(child, {
    stop: async () => { throw new Error("PRIVATE_STOP_ERROR"); }, verify: async () => true,
  });
  await assert.rejects(preservingLaunchFailure(async () => 7, () => observation.cleanup()), {
    code: "OWNED_LAUNCH_CLEANUP_UNPROVED",
  });
  assert.equal(child.kills, 1);
  assert.equal((await observation.diagnostics()).cleanupPhase, "unproved");
});

test("output capture is bounded, counts all bytes and preserves chunked UTF-8", async () => {
  const child = childFixture();
  const observation = observeOwnedLaunch(child, { maxOutputBytes: 16, verify: async () => true });
  const unicode = Buffer.from("é界");
  child.stdout.write(unicode.subarray(0, 1));
  child.stdout.write(unicode.subarray(1));
  assert.equal(observation.output(), "é界");
  child.stdout.write(Buffer.alloc(20));
  child.emit("close", 7, null);
  await assert.rejects(observation.wait(), { code: "OWNED_LAUNCH_OUTPUT_LIMIT" });
  assert.equal((await observation.diagnostics()).stdoutBytes, 25);
  assert.equal(observation.output(), "é界");
  await observation.cleanup();
});
