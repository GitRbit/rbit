// Runtime generations keep Windows executables relinkable and expose active processes.
import { randomUUID } from "node:crypto";
import { hostname } from "node:os";
import { dirname, join } from "node:path";
import { readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { leased, UncertainProcessTreeError } from "./lease.mjs";
import { guarded, managedDirectory } from "./paths.mjs";
import { finishRuntime, protectedRuntime } from "./runtime-marker.mjs";
import { launchTree, stopTree, verifyTreeStopped } from "./process-tree.mjs";

export async function runBinary(ctx, executable, args, env = process.env, {
  cwd = ctx.checkout, stdio = "inherit", launch = launchTree, verify = verifyTreeStopped,
} = {}) {
  await guarded(ctx.root, executable);
  await managedDirectory(ctx, ctx.cache);
  return leased(join(ctx.cache, ".lease"), async () => {
    const marker = await guarded(ctx.root, join(dirname(executable), "running.json"));
    try {
      const previous = JSON.parse(await readFile(marker, "utf8"));
      if (protectedRuntime(previous)) throw new Error("This binary generation is already running or has unverified descendants");
    } catch (error) { if (error.code !== "ENOENT") throw error; }
    const token = randomUUID();
    const stopFile = await guarded(ctx.root, join(dirname(executable), `.stop-${token}`));
    const proofFile = await guarded(ctx.root, join(dirname(executable), `.stopped-${token}`));
    const child = launch(executable, args, { cwd, env, stdio, stopFile, proofFile, token });
    child.runtimeCompletion = new Promise((done, reject) => child.once("close", async () => {
      try {
        const verified = await verify(child);
        await finishRuntime(ctx, marker, token, verified);
        if (!verified) throw new UncertainProcessTreeError("Runtime descendants are unverified; ownership marker retained for inspection");
        done();
      } catch (error) { reject(error); }
    }));
    child.runtimeCompletion.catch((error) => { child.runtimeFailure = error; });
    // Wait for spawn success before publishing ownership, and keep removal excluded.
    await new Promise((done, reject) => { child.once("spawn", done); child.once("error", reject); });
    try {
      await writeFile(marker, JSON.stringify({ pid: child.pid, host: hostname(), token, requiresProof: true }));
      await rm(join(dirname(executable), "handoff.json"), { force: true });
    } catch (error) { await stopTree(child, { awaitMarker: false }); throw error; }
    return child;
  });
}

/** Keep recent handoffs and every active generation. Never delete running binaries. */
export async function pruneGenerations(ctx, keep = 3) {
  await managedDirectory(ctx, ctx.cache);
  return leased(join(ctx.cache, ".lease"), () => pruneInactive(ctx, keep));
}

async function pruneInactive(ctx, keep) {
  const root = join(ctx.state, "bin");
  let generations;
  try { generations = await readdir(root); } catch (error) { if (error.code === "ENOENT") return; throw error; }
  const inactive = [];
  for (const name of generations) {
    const path = await guarded(ctx.root, join(root, name));
    let running = false;
    for (const marker of ["running.json", "handoff.json"]) {
      try {
        const record = JSON.parse(await readFile(join(path, marker), "utf8"));
        running ||= protectedRuntime(record);
      } catch (error) { if (error.code !== "ENOENT") throw error; }
    }
    if (!running) inactive.push({ path, time: (await stat(path)).mtimeMs });
  }
  inactive.sort((a, b) => b.time - a.time);
  for (const entry of inactive.slice(keep)) await rm(entry.path, { recursive: true });
}
