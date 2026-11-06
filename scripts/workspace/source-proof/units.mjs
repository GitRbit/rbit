// Discover actual Cargo layouts. Only validated small freshness stamps are mutable.
import { lstat, opendir, unlink } from "node:fs/promises";
import { basename, join, relative, isAbsolute } from "node:path";
import { guarded } from "../paths.mjs";
import { boundedInput } from "../retention/policy.mjs";
import { hash } from "./inputs.mjs";
import { requireLease } from "../storage.mjs";
const prefixes = ["run-build-script-", "build-script-", "test-integration-test-", "test-example-", "test-bench-",
  "test-lib-", "test-bin-", "lib-", "bin-", "example-", "bench-"];
const hexadecimal = /^[a-f0-9]{16}$/;

export function primaryProfile(lane, profile) {
  const inside = root => {
    const path = relative(root, profile);
    return !isAbsolute(path) && path !== ".." && !path.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`);
  };
  if (inside(join(lane, "intermediates"))) return join(lane, "target", relative(join(lane, "intermediates"), profile));
  if (inside(join(lane, "target"))) return profile;
  throw new Error("SOURCE_PROOF_LAYOUT_UNOBSERVED");
}

async function exists(path) {
  try { return await lstat(path); } catch (error) { if (error.code === "ENOENT") return null; throw error; }
}
async function fileSignature(lane, path, controls, budget, codegen = false, primaryDepExport = false) {
  await guarded(lane, path);
  const info = await lstat(path, { bigint: true });
  if (!info.isFile() || info.isSymbolicLink()) throw new Error("SOURCE_PROOF_ARTIFACT_REFUSED");
  const identity = [String(info.dev), String(info.ino), String(info.size), String(info.mtimeNs), String(info.ctimeNs)];
  let content = null;
  // Codegen policy is size/order independent. Old helpers may rewrite these while
  // preserving Cargo hashes/mtimes; identity plus ctime still changes on real writes.
  const artifact = codegen || /\.(?:rmeta|rlib|exe|pdb|dll|so(?:\.\d+)*|dylib|a|lib|o|obj|dwo)$/i.test(path);
  if (controls || (!artifact && info.size <= 64n * 1024n)) {
    const field = controls ? "controlBytes" : "textBytes";
    budget[field] = (budget[field] ?? 0) + Number(info.size);
    if (info.size > 8n * 1024n ** 2n || budget[field] > 128 * 1024 ** 2) throw new Error(controls ? "SOURCE_PROOF_CONTROL_BOUND" : "SOURCE_PROOF_TEXT_BOUND");
    const bytes = await boundedInput(path, false, 8 * 1024 ** 2);
    content = hash(bytes.toString("base64"));
  }
  const after = await lstat(path, { bigint: true });
  if (hash(identity) !== hash([String(after.dev), String(after.ino), String(after.size), String(after.mtimeNs), String(after.ctimeNs)])) {
    throw new Error("SOURCE_PROOF_ARTIFACT_CHANGED");
  }
  // Cargo regenerates primary external-build .d exports even on some fresh jobs.
  // Their bytes matter, not their rewrite timestamp; actual compiler/fingerprint
  // dep-info and every codegen file retain the full persistent identity above.
  const persistent = primaryDepExport ? ["primary-dep-export", String(info.size)] : identity;
  return [relative(lane, path).replaceAll("\\", "/"), persistent, content];
}

async function profiles(lane) {
  const found = [];
  let directories = 0;
  async function visit(path, depth) {
    if (++directories > 512) throw new Error("SOURCE_PROOF_LAYOUT_BOUND");
    await guarded(lane, path);
    const fingerprint = join(path, ".fingerprint");
    if (await exists(fingerprint)) {
      await guarded(lane, fingerprint);
      if (!(await lstat(fingerprint)).isDirectory()) throw new Error("SOURCE_PROOF_LAYOUT_REFUSED");
      found.push({ path, fingerprint });
      return;
    }
    if (depth >= 8) throw new Error("SOURCE_PROOF_LAYOUT_UNOBSERVED");
    for await (const entry of await opendir(path)) {
      if (entry.isSymbolicLink()) throw new Error("SOURCE_PROOF_LAYOUT_REFUSED");
      if (entry.isDirectory()) await visit(join(path, entry.name), depth + 1);
    }
  }
  for (const name of ["target", "intermediates"]) {
    const path = join(lane, name);
    if (await exists(path)) await visit(path, 0);
  }
  return found;
}

function shape(bytes, runBuildScript) {
  let data;
  try { data = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
  catch { throw new Error("SOURCE_PROOF_CONTROL_REFUSED"); }
  if (!data || !Array.isArray(data.deps) || !Array.isArray(data.local) || !Array.isArray(data.rustflags)
    || typeof data.features !== "string" || typeof data.profile !== "number" || typeof data.target !== "number") {
    throw new Error("SOURCE_PROOF_CONTROL_REFUSED");
  }
  let features;
  try { features = runBuildScript && data.features === "" && data.target === 0 && data.profile === 0
    ? [] : JSON.parse(data.features); } catch { throw new Error("SOURCE_PROOF_CONTROL_REFUSED"); }
  if (!Array.isArray(features) || features.some(value => typeof value !== "string")) throw new Error("SOURCE_PROOF_CONTROL_REFUSED");
  let scriptDependency;
  if (runBuildScript) {
    const matches = [...bytes.toString("utf8").matchAll(/\[\s*\d+\s*,\s*"build_script_build"\s*,\s*(?:false|true)\s*,\s*(\d+)\s*\]/g)];
    if (matches.length !== 1 || BigInt(matches[0][1]) > 0xffffffffffffffffn) throw new Error("SOURCE_PROOF_BUILD_SCRIPT_REFUSED");
    scriptDependency = matches[0][1];
  }
  return { features: features.sort(), scriptDependency };
}

export async function scanUnits(lane, inputs) {
  const units = new Map(), budget = { entries: 0, controlBytes: 0, textBytes: 0 };
  for (const profile of await profiles(lane)) {
    const discovered = [];
    for await (const entry of await opendir(profile.fingerprint)) {
      if (++budget.entries > 30000) throw new Error("SOURCE_PROOF_UNIT_BOUND");
      const match = entry.name.match(/^(.*)-([a-f0-9]{16})$/);
      if (!match && [...inputs.names.keys()].some(name => entry.name.startsWith(`${name}-`))) throw new Error("SOURCE_PROOF_LAYOUT_UNOBSERVED");
      if (!match || !inputs.names.has(match[1])) continue;
      const pkg = inputs.names.get(match[1]);
      const directory = await guarded(lane, join(profile.fingerprint, entry.name));
      if (!entry.isDirectory() || entry.isSymbolicLink()) throw new Error("SOURCE_PROOF_UNIT_REFUSED");
      const files = [];
      for await (const child of await opendir(directory)) {
        if (files.length >= 32 || !child.isFile() || child.isSymbolicLink()) throw new Error("SOURCE_PROOF_CONTROL_REFUSED");
        files.push(child.name);
      }
      const controls = [];
      for (const name of files.sort()) controls.push(await fileSignature(lane, join(directory, name), true, budget));
      for (const stamp of files.filter(name => prefixes.some(prefix => name.startsWith(prefix)) && !name.includes("."))) {
        const prefix = prefixes.find(prefix => stamp.startsWith(prefix));
        const target = stamp.slice(prefix.length);
        if (!/^[A-Za-z0-9_-]{1,128}$/.test(target)) throw new Error("SOURCE_PROOF_CONTROL_REFUSED");
        const path = await guarded(lane, join(directory, stamp));
        const bytes = await boundedInput(path, false, 16);
        // Cargo can leave an empty stamp after a failed/interrupted compilation.
        // It already means dirty: do not approve it or make the lane unusable.
        if (bytes.length === 0) continue;
        if (!hexadecimal.test(bytes.toString("ascii")) || !files.includes(`${stamp}.json`)) throw new Error("SOURCE_PROOF_STAMP_REFUSED");
        const { features, scriptDependency } = shape(await boundedInput(join(directory, `${stamp}.json`), false, 1024 * 1024), prefix === "run-build-script-");
        const id = hash(relative(lane, path).replaceAll("\\", "/"));
        discovered.push({ id, path, directory, profile: profile.path, hash: match[2], pkg, target, prefix, features, controls, companions: [],
          fingerprint: Buffer.from(bytes.toString("ascii"), "hex").readBigUInt64LE().toString(), scriptDependency,
          stampSignature: await fileSignature(lane, path, true, budget) });
      }
    }
    const byHash = new Map();
    for (const unit of discovered) {
      const list = byHash.get(unit.hash) ?? [];
      list.push(unit); byHash.set(unit.hash, list);
    }
    for (const name of ["deps", "examples"]) {
      const directory = join(profile.path, name);
      if (!await exists(directory)) continue;
      await guarded(lane, directory);
      for await (const entry of await opendir(directory)) {
        if (++budget.entries > 150000) throw new Error("SOURCE_PROOF_ARTIFACT_BOUND");
        const match = entry.name.match(/-([a-f0-9]{16})(?:\.|$)/);
        const selected = match && byHash.get(match[1]);
        if (!selected) continue;
        if (!entry.isFile() || entry.isSymbolicLink()) throw new Error("SOURCE_PROOF_ARTIFACT_REFUSED");
        const path = join(directory, entry.name);
        const signature = await fileSignature(lane, path, entry.name.endsWith(".d"), budget, !entry.name.endsWith(".d"));
        for (const unit of selected) unit.companions.push(signature);
      }
    }
    for (const unit of discovered) {
      const build = join(profile.path, "build", basename(unit.directory));
      async function buildOutputs(path, depth = 0) {
        if (++budget.entries > 150000 || depth > 24) throw new Error("SOURCE_PROOF_ARTIFACT_BOUND");
        await guarded(lane, path);
        const info = await lstat(path);
        if (info.isDirectory()) {
          for await (const child of await opendir(path)) await buildOutputs(join(path, child.name), depth + 1);
        } else unit.companions.push(await fileSignature(lane, path, path.endsWith(".d"), budget, unit.prefix === "build-script-"));
      }
      if (unit.prefix.includes("build-script") && await exists(build)) await buildOutputs(build);
      if (unit.prefix === "bin-") {
        const suffix = process.platform === "win32" ? ".exe" : "";
        const primary = primaryProfile(lane, profile.path);
        for (const root of [profile.path, join(profile.path, "deps"), primary]) {
          for (const name of new Set([unit.target, unit.target.replaceAll("-", "_")])) for (const extension of [suffix, ".pdb", ".d"]) {
            const file = join(root, `${name}${extension}`);
            if (await exists(file)) unit.companions.push(await fileSignature(lane, file, extension === ".d", budget,
              extension !== ".d", extension === ".d" && root === primary));
          }
        }
      }
      // Cargo can share unhashed dynamic-library outputs between units.
      for (const directory of [unit.profile, join(unit.profile, "deps"), join(unit.profile, "examples")]) {
        for (const name of [unit.target, `lib${unit.target}`]) for (const extension of [".dll", ".so", ".dylib"]) {
          const path = join(directory, `${name}${extension}`);
          if (await exists(path)) unit.companions.push(await fileSignature(lane, path, false, budget));
        }
      }
      if (unit.prefix === "lib-") {
        const primary = primaryProfile(lane, profile.path);
        for (const root of [profile.path, primary]) {
          for (const name of new Set([unit.target, unit.target.replaceAll("-", "_")])) {
            for (const extension of [".rlib", ".rmeta", ".a", ".lib", ".d"]) for (const prefix of ["", "lib"]) {
              const path = join(root, `${prefix}${name}${extension}`);
              if (await exists(path)) unit.companions.push(await fileSignature(lane, path, extension === ".d", budget,
                false, extension === ".d" && root === primary));
            }
          }
        }
      }
      unit.signature = hash({ controls: unit.controls, companions: unit.companions.sort(([a], [b]) => a.localeCompare(b)) });
      unit.controlSignature = hash(unit.controls);
      units.set(unit.id, unit);
    }
  }
  return units;
}

export async function invalidateStamp(ctx, lane, unit, lease) {
  await requireLease(ctx, lease);
  await guarded(lane, unit.path);
  if (hash(await fileSignature(lane, unit.path, true, { bytes: 0 })) !== hash(unit.stampSignature)) throw new Error("SOURCE_PROOF_STAMP_CHANGED");
  const bytes = await boundedInput(unit.path, false, 16);
  if (!hexadecimal.test(bytes.toString("ascii"))) throw new Error("SOURCE_PROOF_STAMP_REFUSED");
  await guarded(lane, unit.path);
  await unlink(unit.path);
}
