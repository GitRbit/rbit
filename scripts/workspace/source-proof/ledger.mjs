// Authenticated observations, never paths or deletion instructions loaded from JSON.
import { createHmac, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { lstat, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { guarded } from "../paths.mjs";
import { requireLease } from "../storage.mjs";
import { boundedInput } from "../retention/policy.mjs";
import { hash } from "./inputs.mjs";
const MAXIMUM = 8 * 1024 ** 2;
const digest = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const mac = (key, value) => createHmac("sha256", key).update(JSON.stringify(value)).digest("hex");

export async function ledger(ctx, lane, lease) {
  await requireLease(ctx, lease);
  const keyPath = await guarded(ctx.root, join(lane, ".source-proof.key"));
  const file = await guarded(ctx.root, join(lane, ".source-proof.json"));
  let key, existing;
  try { existing = await boundedInput(file, true, MAXIMUM); }
  catch { throw new Error("SOURCE_PROOF_LEDGER_REFUSED"); }
  try { key = await boundedInput(keyPath, false, 32); }
  catch (error) {
    if (error.code !== "ENOENT" || existing) throw new Error("SOURCE_PROOF_KEY_REFUSED");
    key = randomBytes(32);
    await writeFile(keyPath, key, { flag: "wx", mode: 0o600 });
  }
  if (key.length !== 32) throw new Error("SOURCE_PROOF_KEY_REFUSED");
  const scope = hash({ lane, root: ctx.root });
  let entries = {};
  if (existing) {
    let envelope;
    try { envelope = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(existing)); }
    catch { throw new Error("SOURCE_PROOF_LEDGER_REFUSED"); }
    const data = envelope?.data;
    if (!data || Object.keys(envelope).sort().join() !== "data,mac" || data.schema !== 1 || data.scope !== scope
      || !["complete", "pending", "untrusted"].includes(data.status) || !digest(envelope.mac)
      || !timingSafeEqual(Buffer.from(mac(key, data), "hex"), Buffer.from(envelope.mac, "hex"))) throw new Error("SOURCE_PROOF_LEDGER_REFUSED");
    if (!data.entries || typeof data.entries !== "object" || Array.isArray(data.entries) || Object.keys(data.entries).length > 20000
      || Object.entries(data.entries).some(([id, entry]) => !digest(id) || !entry || Object.keys(entry).sort().join() !== "provenance,signature"
        || !digest(entry.provenance) || !digest(entry.signature))) throw new Error("SOURCE_PROOF_LEDGER_REFUSED");
    if (data.status === "complete") entries = data.entries;
  }
  return { ctx, lane, scope, key, file, entries, existed: !!existing };
}

export async function saveLedger(value, status, entries, lease) {
  await requireLease(value.ctx, lease);
  await guarded(value.ctx.root, value.file);
  if (Object.keys(entries).length > 20000) throw new Error("SOURCE_PROOF_LEDGER_BOUND");
  const data = { schema: 1, scope: value.scope, status, entries };
  const bytes = JSON.stringify({ data, mac: mac(value.key, data) }) + "\n";
  if (Buffer.byteLength(bytes) > MAXIMUM) throw new Error("SOURCE_PROOF_LEDGER_BOUND");
  const temporary = await guarded(value.ctx.root, join(value.lane, `.source-proof-${randomUUID()}.tmp`));
  await writeFile(temporary, bytes, { flag: "wx", mode: 0o600 });
  try {
    await requireLease(value.ctx, lease);
    await guarded(value.ctx.root, value.file);
    // Do not replace a newly introduced foreign control file or link.
    try {
      const current = await lstat(value.file);
      if (!current.isFile()) throw new Error("SOURCE_PROOF_LEDGER_REFUSED");
      await ledger(value.ctx, value.lane, lease);
    } catch (error) { if (error.code !== "ENOENT") throw error; }
    await rename(temporary, value.file);
  } finally { await rm(temporary, { force: true }); }
}
