// Explicitly gated Windows native-job test; never invokes Cargo or rustc.
// The reused PowerShell launcher may compile its C# ownership helper with Add-Type.
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { leased, alive } from "../workspace/lease.mjs";
import { ownedCargo, verifiedCargoCompletion } from "../workspace/cargo-tree.mjs";
import { cargoProcess } from "../workspace/build.mjs";

const enabled = process.platform === "win32" && process.env.RBIT_RUN_NATIVE_CARGO_PROOF === "1";

test("native job keeps JSON pipes and proves a leftover compiler-style descendant stopped", {
  skip: !enabled,
  timeout: 30_000,
}, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "rbit-cargo-native-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const ctx = {
    root,
    checkout: root,
    state: join(root, "state"),
    cache: join(root, "cache"),
  };
  await mkdir(ctx.cache);

  // The target exits while its worker remains alive. The native launcher must
  // terminate that worker and prove an empty job before publishing completion.
  const program = `
    const { spawn } = require('node:child_process');
    const { writeFileSync } = require('node:fs');
    const worker = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
    writeFileSync('child.json', JSON.stringify({ pid: worker.pid }));
    process.stdout.write(JSON.stringify({ reason: 'build-finished', success: true }));
    setTimeout(() => process.exit(0), 25);
  `;
  let observedMessage;

  await leased(join(ctx.cache, ".lease"), async (lease) => {
    const receipt = await ownedCargo(ctx, ["-e", program], { ...process.env }, () => {}, {
      lease,
      native: { resolve: async () => process.execPath },
      output: { write: () => {} },
      diagnostics: { write: () => {} },
      captureStderr: true,
      onMessage: (message) => { observedMessage = message; },
    }, (context, args, environment, onArtifact, options) => cargoProcess(
      context,
      args,
      { ...environment, RBIT_WORKSPACE_CARGO_TREE: "off" },
      onArtifact,
      options,
    ));
    assert.equal(verifiedCargoCompletion(receipt, lease), true);
  });

  assert.deepEqual(observedMessage, { reason: "build-finished", success: true });
  const worker = JSON.parse(await readFile(join(root, "child.json"), "utf8"));
  assert.equal(alive(worker.pid), false);
});
