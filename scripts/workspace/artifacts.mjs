// Exact independent executable handoff shared by normal builds and opt-in measurements.
import { randomUUID } from "node:crypto";
import { copyFile, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { guarded, managedDirectory } from "./paths.mjs";

export async function copyExecutables(ctx, lane, namespace, artifacts) {
  const executables = [], copies = [];
  const binaries = new Map();
  for (const item of artifacts.filter((entry) => entry.target.kind.includes("bin"))) {
    const name = basename(item.executable).toLowerCase();
    const previous = binaries.get(name);
    if (previous && (previous.executable !== item.executable || previous.target.name !== item.target.name)) {
      throw new Error(`Ambiguous executable basename ${name}; build one package/target at a time`);
    }
    binaries.set(name, item);
  }
  const generation = await managedDirectory(ctx, join(ctx.state, "bin", randomUUID()));
  for (const item of binaries.values()) {
    await guarded(lane, item.executable);
    const destination = join(generation, basename(item.executable));
    await copyFile(item.executable, destination);
    copies.push({ source: item.executable, destination });
    executables.push({ name: item.target.name, path: destination });
  }
  await writeFile(join(generation, "build.json"), JSON.stringify({ namespace, executables }, null, 2));
  await writeFile(join(generation, "handoff.json"), JSON.stringify({ pid: process.pid, host: (await import("node:os")).hostname() }));
  return { executables, copies };
}
