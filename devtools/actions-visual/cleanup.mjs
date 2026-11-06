import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanupOwned } from "./ownership.mjs";
import { docker } from "./docker.mjs";
const directory = fileURLToPath(new URL("../../.actions-visual-control", import.meta.url));
if (resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 2) throw new Error("No cleanup arguments are accepted");
  console.log(await cleanupOwned(directory, docker));
}
