import { createHash } from "node:crypto";
import { readdir, lstat, readFile } from "node:fs/promises";
import { join } from "node:path";
export const digest = value => createHash("sha256").update(value).digest("hex");
export async function sourceDigest(directory) {
  const files = [];
  async function visit(path, relative = "") {
    for (const name of (await readdir(path)).sort()) {
      const entry = join(path, name), key = relative ? `${relative}/${name}` : name;
      const info = await lstat(entry);
      if (info.isSymbolicLink()) throw new Error("SOURCE_LINK_REFUSED");
      if (info.isDirectory()) await visit(entry, key);
      else if (info.isFile()) files.push([key, digest(await readFile(entry))]);
      else throw new Error("SOURCE_TYPE_REFUSED");
    }
  }
  await visit(directory);
  return digest(JSON.stringify(files));
}
export function revision(value) {
  if (!/^[a-f0-9]{40}$/.test(value ?? "")) throw new Error("REVISION_REQUIRED");
  return value;
}
