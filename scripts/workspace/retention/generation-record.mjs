// Small owned generation records, never promotion/deletion authority loaded from JSON.
import { randomUUID } from "node:crypto";
import { rename, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { guarded } from "../paths.mjs";
import { requireLease } from "../storage.mjs";
import { boundedInput } from "./policy.mjs";
const pause = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));

async function requirePrevious(path, record) {
  let previous;
  try { previous = JSON.parse((await boundedInput(path, false, 65536)).toString("utf8")); }
  catch { throw new Error("GENERATION_RECORD_UNRECOGNIZED"); }
  if (previous.schema !== 1 || previous.source !== "managed-candidate" || previous.namespace !== record.namespace || previous.id !== record.id) {
    throw new Error("GENERATION_RECORD_UNRECOGNIZED");
  }
}

export async function writeGenerationRecord(ctx, path, record, lease, create = false, {
  replace = rename, wait = pause, platform = process.platform,
} = {}) {
  await requireLease(ctx, lease);
  await guarded(ctx.root, path);
  const text = JSON.stringify(record) + "\n";
  if (Buffer.byteLength(text) > 65536) throw new Error("GENERATION_RECORD_BOUND");
  if (create) {
    await writeFile(path, text, { flag: "wx", mode: 0o600 });
    return;
  }
  await requirePrevious(path, record);
  const temporary = await guarded(ctx.root, join(dirname(path), `.generation-${randomUUID()}.tmp`));
  await writeFile(temporary, text, { flag: "wx", mode: 0o600 });
  try {
    for (let attempt = 0; attempt < 3; attempt++) {
      await requireLease(ctx, lease);
      await guarded(ctx.root, path);
      await requirePrevious(path, record);
      try { await replace(temporary, path); return; }
      catch (error) {
        // Windows scanners can briefly deny replacement of a just-read file.
        // Revalidate every retry; never remove the old record as a fallback.
        if (platform !== "win32" || !["EPERM", "EACCES", "EBUSY"].includes(error.code) || attempt === 2) throw error;
        await wait(10);
      }
    }
  } finally { await rm(temporary, { force: true }); }
}
