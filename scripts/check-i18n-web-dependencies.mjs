// Check normal dependencies across all targets, including optional UI features and transitive edges.
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

function forbidden(name) {
  return /^rbit-(?:web|server|services|storage|git|auth|transport|ssh|mail)(?:-|$)/.test(name)
    || name === "rbit-workflow" || name === "serde-saphyr"
    || (name.startsWith("rbit-actions") && name !== "rbit-actions-core")
    || name === "rbit-i18n" || (name.startsWith("rbit-i18n-") && name !== "rbit-i18n-web")
    || /^rbit-plugin-(?:host|actions|i18n|packages)(?:-|$)/.test(name)
    || /^(?:http|hyper|reqwest|axum|tower|sqlx|fluent|intl_pluralrules)(?:-|_|$)/.test(name)
    || /^(?:wasm|wat$|wast$)/.test(name);
}

export function checkI18nWebDependencies(run = spawnSync) {
  const result = run("cargo", [
    "tree", "--color", "never", "--locked", "--package", "rbit-i18n-web", "--all-features", "--target", "all",
    "--edges", "normal", "--prefix", "none",
  ], { encoding: "utf8", maxBuffer: 2 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`Unable to inspect the language UI dependency graph: ${result.error?.message ?? result.stderr ?? "cargo tree failed"}`);
  }
  const lines = result.stdout.trim().split(/\r?\n/);
  if (!/^rbit-i18n-web v\S+(?: |$)/.test(lines[0])) {
    throw new Error("The language UI dependency graph did not contain the expected rbit-i18n-web root.");
  }
  const found = [...new Set(lines.map(line => line.split(" ")[0]).filter(forbidden))].sort();
  if (found.length) {
    throw new Error(`Host or optional plugin implementation leaked into language UI: ${found.join(", ")}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 2) throw new Error("Usage: node scripts/check-i18n-web-dependencies.mjs");
    checkI18nWebDependencies();
    process.stdout.write("Language UI dependency boundary verified (all features, all targets, normal dependencies).\n");
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
