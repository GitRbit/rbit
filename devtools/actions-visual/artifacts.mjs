import { lstat, readdir } from "node:fs/promises";
import { join } from "node:path";
import { artifactBudget } from "./policy.mjs";
export async function verifyArtifacts(directory) {
  const entries = [];
  for (const name of await readdir(directory)) {
    if (!/^(?:[a-z0-9-]+\.png|manifest\.json|container\.json)$/.test(name)) {
      throw new Error("UNEXPECTED_ARTIFACT_NAME");
    }
    const info = await lstat(join(directory, name));
    if (!info.isFile() || info.isSymbolicLink()) throw new Error("UNEXPECTED_ARTIFACT_TYPE");
    entries.push({ name, bytes: info.size });
  }
  if (entries.length > 42) throw new Error("ARTIFACT_COUNT");
  return artifactBudget(entries);
}
if (process.argv[1]?.endsWith("/artifacts.mjs") || process.argv[1]?.endsWith("\\artifacts.mjs")) {
  if (process.argv.length !== 3) throw new Error("Expected artifact directory");
  console.log(JSON.stringify(await verifyArtifacts(process.argv[2])));
}
