import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { cargoProcess } from "../workspace/build.mjs";
import { UncertainProcessTreeError } from "../workspace/lease.mjs";
const artifact = () => ({ reason: "compiler-artifact", target: { kind: ["lib"] } });
test("Cargo hook receives non-executable artifacts and optional diagnostics without global output", async () => {
  const messages = [], chunks = [];
  await cargoProcess({ checkout: "." }, ["check"], { RBIT_WORKSPACE_CARGO_TREE: "off" }, () => assert.fail("not executable"), {
    captureStderr: true, onMessage: (message) => messages.push(message), output: { write: (chunk) => chunks.push(String(chunk)) }, diagnostics: { write: (chunk) => chunks.push(String(chunk)) },
    spawnProcess(_command, _args, config) {
      assert.equal(config.stdio[2], "pipe"); const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
      queueMicrotask(() => { child.stdout.end(`${JSON.stringify(artifact())}\n`); child.stderr.write("captured diagnostic"); child.emit("close", 0, null); }); return child;
    },
  });
  assert.equal(messages.length, 1); assert.deepEqual(chunks, ["captured diagnostic"]);
});

test("exit observations follow JSON drain and completion verification without hiding child failure", async () => {
  for (const code of [0, 101]) {
    const order = [];
    const result = cargoProcess({ checkout: "." }, ["check"], { RBIT_WORKSPACE_CARGO_TREE: "off" }, () => {}, {
      onMessage: () => order.push("message"), verifyCompletion: async () => { order.push("proof"); return {}; },
      onExit: (value, signal) => { assert.equal(value, code); assert.equal(signal, null); order.push("exit"); },
      spawnProcess() {
        const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
        queueMicrotask(() => { child.stdout.end(JSON.stringify(artifact()) + "\n"); child.emit("close", code, null); });
        return child;
      },
    });
    if (code === 0) await result; else await assert.rejects(result, /exited 101/);
    assert.deepEqual(order, ["message", "proof", "exit"]);
  }
});

test("exit-reporting failure cannot replace an earlier process uncertainty", async () => {
  const original = Object.freeze(new UncertainProcessTreeError("synthetic uncertain callback"));
  await assert.rejects(cargoProcess({ checkout: "." }, ["check"], { RBIT_WORKSPACE_CARGO_TREE: "off" }, () => {}, {
    onMessage: () => { throw original; }, verifyCompletion: async () => ({}),
    onExit: () => { throw Error("secondary reporting failure"); },
    spawnProcess() {
      const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
      queueMicrotask(() => { child.stdout.end(JSON.stringify(artifact()) + "\n"); child.emit("close", 0, null); });
      return child;
    },
  }), error => error === original);
});
