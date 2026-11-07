// Runtime ownership outlives a supervisor PID when descendant cleanup is uncertain.
import { readFile, rm, writeFile } from "node:fs/promises";
import { hostname } from "node:os";
import { join } from "node:path";
import { alive, leased } from "./lease.mjs";
import { guarded } from "./paths.mjs";

export function protectedRuntime(record) {
  return record.requiresProof === true || record.uncertain === true || record.host !== hostname() || alive(record.pid);
}

/** Compare nonce and unlink while holding the same lease as every new publication. */
export async function finishRuntime(ctx, marker, token, verified, { remove = rm } = {}) {
  return leased(join(ctx.cache, ".lease"), async () => {
    await guarded(ctx.root, marker);
    let record;
    try { record = JSON.parse(await readFile(marker, "utf8")); }
    catch (error) { if (error.code === "ENOENT") return; throw error; }
    if (record.token !== token) return;
    if (verified) await remove(marker, { force: true });
    else await writeFile(marker, JSON.stringify({ ...record, uncertain: true }));
  });
}
