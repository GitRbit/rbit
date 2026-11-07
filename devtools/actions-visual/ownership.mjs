import { mkdir, readFile, writeFile, lstat } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
const label = "rbit.visual.owner";
export async function createOwnership(directory) {
  await mkdir(directory, { recursive: false, mode: 0o700 });
  const nonce = randomUUID();
  await writeFile(join(directory, "owner.json"), JSON.stringify({ nonce }), { flag: "wx", mode: 0o600 });
  return { nonce, cidfile: join(directory, "container.cid"), label: `${label}=${nonce}` };
}
export async function cleanupOwned(directory, command) {
  try {
    if (!(await lstat(directory)).isDirectory() || (await lstat(directory)).isSymbolicLink()) {
      throw new Error("CONTROL_LINK_REFUSED");
    }
  } catch (error) { if (error.code === "ENOENT") return "not-created"; throw error; }
  const owner = join(directory, "owner.json");
  if ((await lstat(owner)).isSymbolicLink()) throw new Error("CONTROL_LINK_REFUSED");
  const { nonce } = JSON.parse(await readFile(owner, "utf8"));
  if (!/^[a-f0-9-]{36}$/.test(nonce ?? "")) throw new Error("CONTROL_NONCE_INVALID");
  const candidates = new Set();
  try {
    const path = join(directory, "container.cid");
    if ((await lstat(path)).isSymbolicLink()) throw new Error("CONTROL_LINK_REFUSED");
    candidates.add((await readFile(path, "utf8")).trim());
  } catch (error) { if (error.code !== "ENOENT") throw error; }
  // An interrupted create may commit before writing its cidfile. The unique
  // prepublished label permits a narrow owned-container lookup, never a prune.
  const found = await command(["ps", "--all", "--quiet", "--no-trunc", "--filter", `label=${label}=${nonce}`]);
  for (const cid of found.split(/\s+/).filter(Boolean)) candidates.add(cid);
  for (const cid of candidates) {
    if (!/^[a-f0-9]{64}$/.test(cid)) throw new Error("CONTROL_CID_INVALID");
    const exists = await command(["ps", "--all", "--quiet", "--no-trunc", "--filter", `id=${cid}`]);
    if (!exists) continue; // A prior successful cleanup may have removed this CID.
    if (exists !== cid) throw new Error("CONTAINER_ID_MISMATCH");
    const value = await command(["inspect", "--format", `{{ index .Config.Labels "${label}" }}`, cid]);
    if (value !== nonce) throw new Error("CONTAINER_OWNER_MISMATCH");
    await command(["rm", "--force", cid]);
  }
  return "cleaned";
}
