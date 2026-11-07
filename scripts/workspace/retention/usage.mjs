// Bounded streaming metadata observation. No following links, content reads or deletion.
import { lstat, opendir, statfs } from "node:fs/promises";
import { join } from "node:path";
export async function namespaceUsage(path, { maximumEntries = 200_000, maximumDepth = 32 } = {}) {
  let bytes = 0, entries = 0;
  if (!Number.isSafeInteger(maximumEntries) || maximumEntries < 1 || maximumEntries > 200_000
    || !Number.isSafeInteger(maximumDepth) || maximumDepth < 0 || maximumDepth > 32) {
    return { bytes, entries, complete: false };
  }
  async function visit(path, depth) {
    if (++entries > maximumEntries || depth > maximumDepth) throw new Error("USAGE_LIMIT");
    const info = await lstat(path);
    if (info.isSymbolicLink()) throw new Error("USAGE_LINK");
    if (info.isDirectory()) {
      for await (const entry of await opendir(path)) await visit(join(path, entry.name), depth + 1);
    } else if (info.isFile()) {
      bytes += info.size;
      if (!Number.isSafeInteger(bytes)) throw new Error("USAGE_RANGE");
    } else throw new Error("USAGE_SPECIAL_FILE");
  }
  try { await visit(path, 0); return { bytes, entries, complete: true }; }
  catch { return { bytes, entries, complete: false }; }
}
export async function freeBytes(path) {
  try {
    const info = await statfs(path, { bigint: true });
    const bytes = info.bavail * info.bsize;
    return bytes >= 0n && bytes <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(bytes) : null;
  } catch { return null; }
}
