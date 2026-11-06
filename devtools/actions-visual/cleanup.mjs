import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanupOwned } from "./ownership.mjs";
import { docker } from "./docker.mjs";
import { containerMode } from "./production-policy.mjs";
if (resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const mode = containerMode(process.argv.slice(2));
  const directory = fileURLToPath(new URL(mode === "production" ? "../../.actions-production-control" : "../../.actions-visual-control", import.meta.url));
  console.log(await cleanupOwned(directory, docker));
}
