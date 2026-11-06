// Prove the actual copied outputs, never a caller-supplied exact:true observation.
import { createHash } from "node:crypto";
import { open, lstat } from "node:fs/promises";
import { join } from "node:path";
import { guarded } from "../paths.mjs";
// The IO seam supports deterministic file-growth fixtures. proveHandoff always
// invokes this helper with real filesystem operations, never caller-supplied IO.
export async function fingerprint(path, { stat = lstat, openFile = open } = {}) {
  const before = await stat(path);
  if (!before.isFile() || before.isSymbolicLink() || !Number.isSafeInteger(before.size)
    || before.size < 0 || before.size >= Number.MAX_SAFE_INTEGER) throw new Error("HANDOFF_FILE_REFUSED");
  const file = await openFile(path, "r");
  try {
    const hash = createHash("sha256");
    const buffer = Buffer.alloc(64 * 1024);
    let total = 0;
    const limit = before.size + 1;
    while (total < limit) {
      const requested = Math.min(buffer.length, limit - total);
      const { bytesRead } = await file.read(buffer, 0, requested, null);
      if (!Number.isSafeInteger(bytesRead) || bytesRead < 0 || bytesRead > requested) throw new Error("HANDOFF_CHANGED");
      if (!bytesRead) break;
      total += bytesRead;
      hash.update(buffer.subarray(0, bytesRead));
    }
    const after = await file.stat();
    if (total !== before.size || before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ino !== after.ino || before.dev !== after.dev) throw new Error("HANDOFF_CHANGED");
    return { size: after.size, dev: after.dev, ino: after.ino, digest: hash.digest("hex") };
  } finally { await file.close(); }
}
export async function proveHandoff(ctx, lane, copies) {
  if (!Array.isArray(copies) || copies.length > 256) throw new Error("HANDOFF_BOUND");
  const fingerprints = [];
  for (const copy of copies) {
    await guarded(lane, copy.source);
    await guarded(join(ctx.state, "bin"), copy.destination);
    const [source, destination] = await Promise.all([fingerprint(copy.source), fingerprint(copy.destination)]);
    if (source.size !== destination.size || source.digest !== destination.digest
      || (source.dev === destination.dev && source.ino === destination.ino)) throw new Error("HANDOFF_NOT_INDEPENDENT");
    fingerprints.push({ bytes: destination.size, sha256: destination.digest });
  }
  return { complete: true, exact: true, files: fingerprints };
}
