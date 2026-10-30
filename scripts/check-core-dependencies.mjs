// Verify the shipped minimal server graph, rather than the union of workspace features.
import { spawnSync } from "node:child_process";

const result = spawnSync(
  "cargo",
  ["tree", "--color", "never", "--locked", "--package", "rbit-server", "--no-default-features", "--edges", "normal", "--prefix", "none"],
  { encoding: "utf8", maxBuffer: 2 * 1024 * 1024 },
);
if (result.error || result.status !== 0) {
  process.stderr.write(result.error?.message ?? result.stderr ?? "Unable to inspect the core dependency graph.\n");
  process.exit(1);
}
const forbidden = new Set(["rbit-actions", "rbit-workflow", "serde-saphyr"]);
if (!result.stdout.startsWith("rbit-server ")) {
  process.stderr.write("The core dependency graph did not contain the expected server root.\n");
  process.exit(1);
}
const found = new Set(result.stdout.split("\n").map((line) => line.split(" ")[0]).filter((name) => forbidden.has(name)));
if (found.size) {
  process.stderr.write(`Optional Actions implementations leaked into the core server: ${[...found].join(", ")}\n`);
  process.exit(1);
}
process.stdout.write("Core server excludes Actions implementation and workflow parser dependencies.\n");
