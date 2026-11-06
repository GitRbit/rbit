// Reports contain repository-relative names and hashes, never full environment/paths.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, readFile, readdir } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { homedir } from "node:os";
import { guarded, git } from "../workspace/paths.mjs";

export const sha = (value) => createHash("sha256").update(value).digest("hex");

async function files(root, path) {
  await guarded(root, path);
  const info = await lstat(path);
  if (info.isSymbolicLink()) throw new Error("Linked provenance source refused");
  if (info.isDirectory()) return (await Promise.all((await readdir(path)).sort().map((entry) => files(root, join(path, entry))))).flat();
  return [{ file: relative(root, path).replaceAll("\\", "/"), sha256: sha(await readFile(path)) }];
}

export async function sourceIdentity(ctx) {
  const entries = (await Promise.all(["tools/render-probe", "scripts/render-compile", "scripts/rewrite/measure-render-compile.mjs", "scripts/workspace", "Cargo.toml", "Cargo.lock", ".cargo/config.toml"].map((path) => files(ctx.checkout, join(ctx.checkout, path))))).flat();
  const configs = await cargoConfigIdentity(ctx, process.env);
  return { revision: git(ctx.checkout, ["rev-parse", "HEAD"]), files: entries, cargoConfigs: configs, fingerprint: sha(JSON.stringify({ entries, configs })),
    checkoutIdentity: sha(ctx.checkout), lockSha256: entries.find((entry) => entry.file === "Cargo.lock").sha256 };
}

export function flagIdentity(env) {
  const selected = Object.entries(env).filter(([name]) => /^(RUST.*FLAGS|CARGO_.*FLAGS|CARGO_PROFILE_.*|CARGO_BUILD_TARGET|CC|CXX|CFLAGS|LIBSQLITE3_FLAGS)$/.test(name)).sort(([a], [b]) => a.localeCompare(b));
  return { effectiveFlagsSha256: sha(JSON.stringify(selected)), environmentSha256: sha(JSON.stringify(Object.entries(env).sort(([a], [b]) => a.localeCompare(b)))), wrappers: "explicitly disabled", incremental: false, automaticCacheMaintenance: false };
}

export async function cargoConfigIdentity(ctx, env) {
  const locations = [];
  for (let path = ctx.checkout; ; path = dirname(path)) {
    locations.push(join(path, ".cargo")); if (dirname(path) === path) break;
  }
  locations.push(env.CARGO_HOME ? resolve(ctx.checkout, env.CARGO_HOME) : join(homedir(), ".cargo"));
  const entries = [];
  for (const [scope, path] of [...new Set(locations)].entries()) for (const name of ["config", "config.toml"]) {
    try { entries.push({ scope, name, sha256: sha(await readFile(join(path, name))) }); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  return entries;
}

export function toolIdentity(ctx, env) {
  const run = (name, args) => execFileSync(name, args, { cwd: ctx.checkout, env, encoding: "utf8", windowsHide: true, maxBuffer: 8 * 1024 * 1024 }).trim();
  const metadata = JSON.parse(run("cargo", ["metadata", "--no-deps", "--locked", "--format-version=1"]));
  return { tools: { rustc: run(env.RUSTC || "rustc", ["-Vv"]), cargo: run("cargo", ["-V"]) },
    packages: new Map(metadata.packages.map((pkg) => [pkg.id, { name: pkg.name, version: pkg.version }])) };
}

export function artifactIdentity(message, packages = new Map()) {
  if (message.reason !== "compiler-artifact") return null;
  const suffix = message.package_id?.split("#").at(-1);
  const match = suffix?.match(/^([a-zA-Z0-9_-]+)@([a-zA-Z0-9.+_-]+)$/);
  const pkg = packages.get(message.package_id) ?? (match && { name: match[1], version: match[2] });
  const kinds = message.target?.kind;
  if (!pkg || !/^[a-zA-Z0-9_-]+$/.test(pkg.name) || !/^[a-zA-Z0-9.+_-]+$/.test(pkg.version) || typeof message.fresh !== "boolean"
    || !Array.isArray(message.features) || message.features.some((feature) => !/^[a-zA-Z0-9_.+-]+$/.test(feature))
    || !Array.isArray(kinds) || kinds.some((kind) => !["lib", "rlib", "bin", "proc-macro", "custom-build", "cdylib", "staticlib", "dylib", "example", "test", "bench"].includes(kind))) return { invalid: true };
  const profile = {};
  for (const field of ["opt_level", "debuginfo", "debug_assertions", "overflow_checks", "test"]) {
    const value = message.profile?.[field];
    if ((field === "debuginfo" && value === null) || typeof value === "boolean" || (Number.isInteger(value) && value >= 0 && value <= 3) || ["0", "1", "2", "3", "s", "z", "none", "limited", "full", "line-tables-only"].includes(value)) profile[field] = value;
    else if (value !== undefined) return { invalid: true };
  }
  return { package: pkg.name, version: pkg.version, sourceIdentity: sha(message.package_id.split("#")[0]),
    features: [...message.features].sort(), targetKinds: kinds, profile, fresh: message.fresh === true };
}

export function sampleValidity(artifacts, job, exit, sourceBefore, sourceAfter, memory) {
  const reasons = [];
  if (exit?.code !== 0 || exit?.signal) reasons.push("compiler-child-failed");
  if (sourceBefore.fingerprint !== sourceAfter.fingerprint) reasons.push("source-or-lock-changed");
  if (artifacts.some((artifact) => artifact.invalid)) reasons.push("unrecognized-artifact-identity");
  const probe = artifacts.filter((artifact) => artifact.package === "rbit-render-probe" && artifact.targetKinds?.includes("lib"));
  if (!probe.length || probe.some((artifact) => artifact.fresh)) reasons.push("cold-probe-work-not-proved");
  const expected = [job.mode, `count-${job.count}`].sort().join(",");
  if (probe.some((artifact) => artifact.features.join(",") !== expected)) reasons.push("probe-features-not-isolated");
  if (artifacts.some((artifact) => artifact.package !== "rbit-render-probe" && artifact.fresh === false)) reasons.push("dependency-compiled-during-timed-sample");
  if (memory.reasons.includes("external-compiler-observed")) reasons.push("external-compiler-contamination");
  return { compileComparable: reasons.length === 0, memoryComparable: reasons.length === 0 && memory.reasons.length === 0, reasons };
}
