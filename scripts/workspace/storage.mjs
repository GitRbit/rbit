// Bounded cache maintenance; source checkouts and runtime data are never cache entries.
import { lstat, readdir, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { guarded, managedDirectory } from "./paths.mjs";
import { leased } from "./lease.mjs";

export async function treeBytes(path) {
  const stat = await lstat(path);
  if (stat.isSymbolicLink()) throw new Error(`Refusing linked cache entry: ${path}`);
  if (!stat.isDirectory()) return stat.size;
  let total = 0;
  for (const entry of await readdir(path)) total += await treeBytes(join(path, entry));
  return total;
}

export async function cacheReport(ctx) {
  await managedDirectory(ctx, ctx.cache);
  const entries = [];
  for (const name of await readdir(ctx.cache)) {
    if (!/^[a-f0-9]{20}$/.test(name)) continue;
    const path = await guarded(ctx.root, join(ctx.cache, name));
    const bytes = await treeBytes(path);
    let used = 0;
    try { used = Number(await readFile(join(path, "last-used"), "utf8")) || 0; }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    entries.push({ name, bytes, used });
  }
  return { bytes: entries.reduce((sum, entry) => sum + entry.bytes, 0), entries };
}

export async function prune(ctx, maxBytes) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) throw new Error("max-bytes must be a nonnegative integer");
  await managedDirectory(ctx, ctx.cache);
  // Never wait then unexpectedly erase an active caller's cache: fail immediately.
  return leased(join(ctx.cache, ".lease"), async () => {
    const report = await cacheReport(ctx);
    const removed = [];
    for (const entry of report.entries.sort((a, b) => a.used - b.used)) {
      if (report.bytes <= maxBytes) break;
      const path = await guarded(ctx.root, join(ctx.cache, entry.name));
      // Revalidate descendants immediately before recursive removal.
      await treeBytes(path);
      await rm(path, { recursive: true });
      report.bytes -= entry.bytes;
      removed.push(entry);
    }
    return { bytes: report.bytes, removed };
  }, { timeout: 0 });
}
