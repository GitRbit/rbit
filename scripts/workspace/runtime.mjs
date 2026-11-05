// Runtime generations keep Windows executables relinkable and expose active processes.
import { spawn } from "node:child_process";
import { hostname } from "node:os";
import { dirname, join } from "node:path";
import { readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { alive, leased } from "./lease.mjs";
import { guarded, managedDirectory } from "./paths.mjs";

export async function runBinary(ctx, executable, args, env = process.env, { cwd = ctx.checkout, stdio = "inherit" } = {}) {
  await guarded(ctx.root, executable);
  await managedDirectory(ctx, ctx.cache);
  return leased(join(ctx.cache, ".lease"), async () => {
    const marker = join(dirname(executable), "running.json");
    try {
      const previous = JSON.parse(await readFile(marker, "utf8"));
      if (previous.host !== hostname() || alive(previous.pid)) throw new Error("This binary generation is already running");
    } catch (error) { if (error.code !== "ENOENT") throw error; }
    const child = spawn(executable, args, { cwd, env, stdio, windowsHide: true });
    // Wait for spawn success before publishing ownership, and keep removal excluded.
    await new Promise((done, reject) => { child.once("spawn", done); child.once("error", reject); });
    try {
      await writeFile(marker, JSON.stringify({ pid: child.pid, host: hostname() }));
      await rm(join(dirname(executable), "handoff.json"), { force: true });
    } catch (error) { child.kill(); throw error; }
    child.once("exit", () => { rm(marker, { force: true }).catch(() => {}); });
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
        running ||= record.host !== hostname() || alive(record.pid);
      } catch (error) { if (error.code !== "ENOENT") throw error; }
    }
    if (!running) inactive.push({ path, time: (await stat(path)).mtimeMs });
  }
  inactive.sort((a, b) => b.time - a.time);
  for (const entry of inactive.slice(keep)) await rm(entry.path, { recursive: true });
}
