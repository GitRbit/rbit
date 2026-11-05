/** Compile the development-only AST checker through the shared lane, then check new crates. */
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, relative } from "node:path";
import { context } from "../workspace/paths.mjs";
import { cargo } from "../workspace/build.mjs";
import { pruneGenerations, runBinary } from "../workspace/runtime.mjs";

async function main() {
  const ctx = await context();
  const baseline = JSON.parse(await readFile(new URL("../../docs/design/rewrite/main-inventory.json", import.meta.url), "utf8"));
  const legacy = new Set(baseline.crates.map((crate) => crate.name));
  // Metadata with --locked is read-only and does not invoke compiler/build-script writers.
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--locked", "--offline", "--format-version=1"], {
    cwd: ctx.checkout, encoding: "utf8", windowsHide: true, stdio: ["pipe", "pipe", "pipe"], maxBuffer: 8 * 1024 * 1024,
  }));
  const roots = metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id) && !legacy.has(pkg.name))
    .map((pkg) => relative(ctx.checkout, dirname(pkg.manifest_path)));
  if (!roots.length) throw new Error("No strict replacement crates found");
  const build = await cargo(ctx, "build", ["--package", "rbit-rewrite-check", "--locked"]);
  const binary = build.executables.find((item) => item.name === "rbit-rewrite-check");
  if (!binary) throw new Error("Cargo did not produce the structural checker");
  const child = await runBinary(ctx, binary.path, roots);
  const code = child.exitCode ?? await new Promise((done) => child.once("exit", (value) => done(value ?? 1)));
  await pruneGenerations(ctx);
  process.exitCode = code;
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
