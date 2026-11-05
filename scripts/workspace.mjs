#!/usr/bin/env node
// All shared-output Cargo writers must enter this command's lease.
import { context } from "./workspace/paths.mjs";
import { cargo } from "./workspace/build.mjs";
import { create, list, remove } from "./workspace/worktrees.mjs";
import { cacheReport, prune } from "./workspace/storage.mjs";
import { pruneGenerations, runBinary } from "./workspace/runtime.mjs";
import { runtimeEnvironment } from "./workspace/ports.mjs";
import { stopTree } from "./workspace/process-tree.mjs";

async function main() {
  const [command, ...args] = process.argv.slice(2);
  const arities = { new: [1, 2], list: [0, 0], remove: [1, 1], "cache-report": [0, 0], "cache-prune": [1, 1] };
  if (arities[command] && (args.length < arities[command][0] || args.length > arities[command][1])) throw new Error(`Unexpected arguments for ${command}`);
  if (command === "cache-prune" && !/^\d+$/.test(args[0])) throw new Error("cache-prune requires an integer byte budget");
  const ctx = await context();
  let result;
  if (command === "new") result = await create(ctx, args[0], args[1]);
  else if (command === "list") result = list(ctx);
  else if (command === "remove") result = await remove(ctx, args[0]);
  else if (command === "cache-report") result = await cacheReport(ctx);
  else if (command === "cache-prune") result = await prune(ctx, Number(args[0]));
  else if (command === "ide-check") {
    await cargo(ctx, "check", args, { env: { ...process.env, RBIT_WORKSPACE_JSON: "1" } });
  } else if (["build", "check", "clippy", "test", "nextest", "generate-lockfile"].includes(command)) {
    result = await cargo(ctx, command, args);
    // Environment may contain secrets: output only deliberate public build metadata.
    result = { namespace: result.namespace, executables: result.executables };
  } else if (command === "run") {
    const separator = args.indexOf("--");
    const buildArgs = separator < 0 ? args : args.slice(0, separator);
    const runArgs = separator < 0 ? [] : args.slice(separator + 1);
    const build = await cargo(ctx, "build", ["--package", "rbit-server", ...buildArgs]);
    const binary = build.executables.find((item) => item.name === "rbit");
    if (!binary) throw new Error("Cargo did not produce rbit");
    const child = await runBinary(ctx, binary.path, runArgs.length ? runArgs : ["serve"], await runtimeEnvironment(ctx));
    child.on("exit", (code) => { process.exitCode = code ?? 1; });
    child.runtimeCompletion.catch((error) => { console.error(error.message); process.exitCode = 1; });
    if (child.exitCode !== null) process.exitCode = child.exitCode;
    let stopping = false;
    for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => {
      if (stopping) return;
      stopping = true;
      stopTree(child).catch((error) => { console.error(error.message); process.exitCode = 1; });
    });
  } else throw new Error("Usage: workspace.mjs new NAME [REV] | list | remove NAME | build/check/clippy/test/nextest [ARGS] | run [BUILD ARGS] -- [RUN ARGS] | cache-report | cache-prune MAX_BYTES");
  // Small inactive binary copies are pruned only after successful commands.
  await pruneGenerations(ctx);
  if (result) process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
