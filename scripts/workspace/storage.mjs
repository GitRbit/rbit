// Bounded cache maintenance; source checkouts and runtime data are never cache entries.
import { lstat, readdir, readFile, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { hostname } from "node:os";
import { guarded, managedDirectory } from "./paths.mjs";
import { leased, owner } from "./lease.mjs";

export const DEFAULT_CACHE_MAX_BYTES = 16 * 1024 ** 3;

/** Zero disables automatic maintenance; manual prune(0) still empties the cache. */
export function cacheBudget(env = process.env) {
  const value = env.RBIT_BUILD_CACHE_MAX_BYTES;
  if (value === undefined) return DEFAULT_CACHE_MAX_BYTES;
  if (typeof value !== "string" || !/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) {
    throw new Error("RBIT_BUILD_CACHE_MAX_BYTES must be a nonnegative safe integer byte count; 0 disables automatic pruning");
  }
  return Number(value);
}

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

async function requireLease(ctx, lease) {
  const path = join(ctx.cache, ".lease");
  const current = await owner(path);
  if (typeof lease?.path !== "string" || typeof lease.token !== "string" || resolve(lease.path) !== resolve(path) || current?.token !== lease.token || current?.pid !== process.pid || current?.host !== hostname()) {
    throw new Error("Cache maintenance requires the current management lease");
  }
  try { await readFile(join(path, "blocked.json")); throw new Error("Blocked lease cannot prune cache"); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
}

async function pruneOwned(ctx, maxBytes, protectedNames, lease) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) throw new Error("max-bytes must be a nonnegative integer");
  await requireLease(ctx, lease);
  const report = await cacheReport(ctx);
  const removed = [];
  for (const entry of report.entries.sort((a, b) => a.used - b.used || a.name.localeCompare(b.name))) {
    if (report.bytes <= maxBytes) break;
    if (protectedNames.has(entry.name)) continue;
    await requireLease(ctx, lease);
    const path = await guarded(ctx.root, join(ctx.cache, entry.name));
    // Revalidate descendants immediately before recursive removal.
    await treeBytes(path);
    await rm(path, { recursive: true });
    report.bytes -= entry.bytes;
    removed.push(entry);
  }
  return { bytes: report.bytes, removed };
}

/** Automatic post-command policy. Caller must still own the build/test lease. */
export async function enforceHighWater(ctx, maxBytes, currentNamespace, lease) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) throw new Error("max-bytes must be a nonnegative integer");
  if (!/^[a-f0-9]{20}$/.test(currentNamespace)) throw new Error("Invalid protected compatibility namespace");
  await requireLease(ctx, lease);
  if (maxBytes === 0) return { disabled: true, maxBytes: 0 };
  const report = await pruneOwned(ctx, maxBytes, new Set([currentNamespace]), lease);
  return { ...report, maxBytes, protectedNamespace: currentNamespace, overageBytes: Math.max(0, report.bytes - maxBytes), disabled: false };
}

export async function prune(ctx, maxBytes) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) throw new Error("max-bytes must be a nonnegative integer");
  await managedDirectory(ctx, ctx.cache);
  // Never wait then unexpectedly erase an active caller's cache: fail immediately.
  return leased(join(ctx.cache, ".lease"), (lease) => pruneOwned(ctx, maxBytes, new Set(), lease), { timeout: 0 });
}
