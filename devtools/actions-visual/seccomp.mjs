// Reconstruct the exact upstream policy without hiding syscall data in long lines.
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
const directory = new URL("./seccomp/", import.meta.url);
export async function seccompPolicy() {
  const read = async name => JSON.parse(await readFile(new URL(name, directory), "utf8"));
  const [archMap, common, standard, capability] = await Promise.all([
    read("architecture.json"), read("common-syscalls.json"),
    read("standard-syscalls.json"), read("capability-syscalls.json"),
  ]);
  standard[1].names = common;
  const policy = { defaultAction: "SCMP_ACT_ERRNO", archMap, syscalls: [...standard, ...capability] };
  const expected = (await readFile(new URL("upstream.sha256", directory), "utf8")).trim();
  if (createHash("sha256").update(JSON.stringify(policy)).digest("hex") !== expected) {
    throw new Error("Vendored seccomp policy differs from reviewed upstream");
  }
  return policy;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) throw new Error("Expected one output file");
  await writeFile(process.argv[2], JSON.stringify(await seccompPolicy()), { flag: "wx" });
}
