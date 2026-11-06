import assert from "node:assert/strict";
import test from "node:test";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { cargoProcess } from "../workspace/build.mjs";
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
