// Check normal dependencies across all targets, including optional UI features and transitive edges.
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

function forbidden(name) {
  return /^rbit-(?:web|server|services|storage|git|auth|transport|ssh|mail)(?:-|$)/.test(name)
    || name === "rbit-workflow" || name === "serde-saphyr"
    || (name.startsWith("rbit-actions") && !["rbit-actions-core", "rbit-actions-web"].includes(name))
    || name === "rbit-i18n" || name.startsWith("rbit-i18n-")
    || /^rbit-plugin-(?:host|actions|i18n|packages)(?:-|$)/.test(name)
    || /^(?:http|hyper|reqwest|axum|tower|sqlx|fluent|intl_pluralrules)(?:-|_|$)/.test(name)
    || /^(?:wasm|wat$|wast$)/.test(name);
}

export function checkActionsWebDependencies(run = spawnSync) {
  const result = run("cargo", [
    "tree", "--color", "never", "--locked", "--package", "rbit-actions-web", "--all-features", "--target", "all",
    "--edges", "normal", "--prefix", "none",
  ], { encoding: "utf8", maxBuffer: 2 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`Unable to inspect the Actions UI dependency graph: ${result.error?.message ?? result.stderr ?? "cargo tree failed"}`);
  }
  const lines = result.stdout.trim().split(/\r?\n/);
  if (!/^rbit-actions-web v\S+(?: |$)/.test(lines[0])) {
    throw new Error("The Actions UI dependency graph did not contain the expected rbit-actions-web root.");
  }
  const found = [...new Set(lines.map(line => line.split(" ")[0]).filter(forbidden))].sort();
  if (found.length) {
    throw new Error(`Host or optional plugin implementation leaked into Actions UI: ${found.join(", ")}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 2) throw new Error("Usage: node scripts/check-actions-web-dependencies.mjs");
    checkActionsWebDependencies();
    process.stdout.write("Actions UI dependency boundary verified (all features, all targets, normal dependencies).\n");
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
