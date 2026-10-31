// Verify the shipped minimal server graph, rather than the union of workspace features.
import { spawnSync } from "node:child_process";

const profile = process.argv[2] ?? "none";
if (!["none", "actions", "i18n"].includes(profile) || process.argv.length > 3) {
  process.stderr.write("Usage: node scripts/check-core-dependencies.mjs [none|actions|i18n]\n");
  process.exit(1);
}
const featureArgs = profile === "none" ? [] : ["--features", profile];
const result = spawnSync(
  "cargo",
  ["tree", "--color", "never", "--locked", "--package", "rbit-server", "--no-default-features", ...featureArgs, "--edges", "normal", "--prefix", "none"],
  { encoding: "utf8", maxBuffer: 2 * 1024 * 1024 },
);
if (result.error || result.status !== 0) {
  process.stderr.write(result.error?.message ?? result.stderr ?? "Unable to inspect the core dependency graph.\n");
  process.exit(1);
}
const actions = ["rbit-actions", "rbit-workflow", "serde-saphyr"];
const catalogs = ["rbit-i18n", "fluent-bundle", "fluent-syntax", "intl_pluralrules"];
const forbidden = new Set(profile === "actions" ? catalogs : profile === "i18n" ? actions : [...actions, ...catalogs]);
if (!result.stdout.startsWith("rbit-server ")) {
  process.stderr.write("The core dependency graph did not contain the expected server root.\n");
  process.exit(1);
}
const found = new Set(result.stdout.split("\n").map((line) => line.split(" ")[0]).filter((name) => forbidden.has(name)));
if (found.size) {
  process.stderr.write(`Optional plugin implementations leaked into the core server: ${[...found].join(", ")}\n`);
  process.exit(1);
}
process.stdout.write(`${profile === "none" ? "Core" : profile + "-only"} server dependency boundary verified.\n`);
