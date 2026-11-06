// Disk-derived local-package inputs. Nothing read here authorizes a cache mutation.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, opendir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { childEnvironment } from "../child-environment.mjs";
import { guarded, contains } from "../paths.mjs";
import { boundedInput, policySnapshot } from "../retention/policy.mjs";
export const hash = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");

export function cargoMetadata(ctx, env) {
  const bytes = execFileSync("cargo", ["metadata", "--locked", "--offline", "--all-features", "--format-version=1"], {
    cwd: ctx.checkout, env: childEnvironment(env), windowsHide: true, maxBuffer: 32 * 1024 ** 2, stdio: ["ignore", "pipe", "pipe"],
  });
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}

const identity = info => [String(info.dev), String(info.ino), String(info.size), String(info.mtimeNs), String(info.ctimeNs)];
export async function absentInput(path, budget, { stat = lstat } = {}) {
  budget.stability.push([hash(path), "absent"]);
  let parent = dirname(path);
  for (let depth = 0; depth < 128; depth++) {
    try { budget.stability.push([hash(parent), identity(await stat(parent, { bigint: true }))]); return; }
    catch (error) {
      if (error.code !== "ENOENT") throw error;
      const next = dirname(parent);
      if (next === parent) break;
      parent = next;
    }
  }
  throw new Error("SOURCE_PROOF_ABSENCE_UNOBSERVED");
}
async function sourceBytes(path, budget) {
  const before = await lstat(path, { bigint: true });
  const bytes = await boundedInput(path, false, 32 * 1024 ** 2);
  const after = await lstat(path, { bigint: true });
  if (hash(identity(before)) !== hash(identity(after))) throw new Error("SOURCE_PROOF_INPUT_CHANGED");
  budget.stability.push([hash(path), identity(after)]);
  return bytes;
}
async function packageFiles(root, budget) {
  const entries = [];
  async function visit(path, depth) {
    if (++budget.entries > 40000 || depth > 32) throw new Error("SOURCE_PROOF_INPUT_BOUND");
    await guarded(root, path);
    const info = await lstat(path);
    if (info.isDirectory()) {
      budget.stability.push([hash(path), identity(await lstat(path, { bigint: true }))]);
      for await (const entry of await opendir(path)) {
        if (entry.name === ".git") continue;
        await visit(join(path, entry.name), depth + 1);
      }
    } else if (info.isFile()) {
      if (info.size > 32 * 1024 ** 2 || (budget.bytes += info.size) > 512 * 1024 ** 2) throw new Error("SOURCE_PROOF_INPUT_BOUND");
      const bytes = await sourceBytes(path, budget);
      entries.push([relative(root, path).replaceAll("\\", "/"), createHash("sha256").update(bytes).digest("hex")]);
    } else throw new Error("SOURCE_PROOF_INPUT_REFUSED");
  }
  budget.stability.push([hash(root), identity(await lstat(root, { bigint: true }))]);
  for await (const entry of await opendir(root)) {
    if (entry.name !== ".git") await visit(join(root, entry.name), 0);
  }
  return hash(entries.sort(([a], [b]) => a.localeCompare(b)));
}

/** Metadata is a bounded Cargo-output seam for temporary tests, never ledger data. */
export async function inputSnapshot(ctx, namespace, command, args, env,
  { metadata = cargoMetadata, readPolicy, absentPolicy = absentInput } = {}, stage = "admission") {
  if (!["admission", "completion"].includes(stage)) throw new Error("SOURCE_PROOF_STAGE_REFUSED");
  const readMetadata = async () => {
    const data = await metadata(ctx, env);
    if (!data || !Array.isArray(data.packages) || data.packages.length > 4096 || !Array.isArray(data.resolve?.nodes)) {
      throw new Error("SOURCE_PROOF_METADATA_UNOBSERVED");
    }
    return data;
  };
  // Admission absorbs its own metadata maintenance before the native interval.
  // Verified completion seals that interval BEFORE metadata can flush Cargo's
  // global-cache database and change absent configuration's ancestor timestamps.
  let data = stage === "admission" ? await readMetadata() : undefined;
  const budget = { entries: 0, bytes: 0, stability: [] };
  const policy = await policySnapshot(ctx, namespace, command, args, env, async (path, optional, limit) => {
    const bytes = await (readPolicy ?? boundedInput)(path, optional, limit);
    if (bytes) budget.stability.push([hash(path), identity(await lstat(path, { bigint: true }))]);
    else await absentPolicy(path, budget);
    return bytes;
  });
  if (!policy.inputId) throw new Error("SOURCE_PROOF_POLICY_UNOBSERVED");
  if (stage === "completion") data = await readMetadata();
  const locals = new Map(), names = new Map(), registryNames = new Set();
  for (const pkg of data.packages) {
    if (typeof pkg.id !== "string" || typeof pkg.name !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(pkg.name)) throw new Error("SOURCE_PROOF_PACKAGE_REFUSED");
    if (pkg.source !== null) { registryNames.add(pkg.name); continue; }
    if (!isAbsolute(pkg.manifest_path ?? "") || !Array.isArray(pkg.targets)) throw new Error("SOURCE_PROOF_PACKAGE_REFUSED");
    const manifest = resolve(pkg.manifest_path);
    const root = await realpath(dirname(manifest));
    await guarded(root, manifest);
    if (!(await lstat(manifest)).isFile()) throw new Error("SOURCE_PROOF_PACKAGE_REFUSED");
    const targets = [];
    for (const target of pkg.targets) {
      if (!/^[A-Za-z0-9_-]{1,128}$/.test(target.name ?? "") || !isAbsolute(target.src_path ?? "")) throw new Error("SOURCE_PROOF_TARGET_REFUSED");
      const path = await guarded(root, target.src_path);
      targets.push([target.name, path]);
    }
    if (locals.has(pkg.id) || names.has(pkg.name)) throw new Error("SOURCE_PROOF_PACKAGE_AMBIGUOUS");
    const identity = hash({ id: pkg.id, root, manifest, targets, policy: policy.inputId });
    const item = { id: pkg.id, name: pkg.name, root, targets, identity, content: await packageFiles(root, budget) };
    locals.set(pkg.id, item); names.set(pkg.name, item);
    if (locals.size > 256) throw new Error("SOURCE_PROOF_PACKAGE_BOUND");
  }
  if (!locals.size || [...names.keys()].some(name => registryNames.has(name))) throw new Error("SOURCE_PROOF_PACKAGE_AMBIGUOUS");
  const nodes = new Map(data.resolve.nodes.map(node => [node.id, node.dependencies]));
  if (nodes.size > 4096 || [...nodes.values()].some(deps => !Array.isArray(deps) || deps.length > 4096 || deps.some(id => typeof id !== "string"))) {
    throw new Error("SOURCE_PROOF_DEPENDENCIES_UNOBSERVED");
  }
  const listed = execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
    cwd: ctx.checkout, encoding: "utf8", maxBuffer: 4 * 1024 ** 2, windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
  }).split("\0").filter(Boolean);
  const shared = [];
  for (const name of [...new Set(listed)].sort()) {
    const path = await guarded(ctx.checkout, resolve(ctx.checkout, name));
    if ([...locals.values()].some(item => contains(item.root, path))) continue;
    if (++budget.entries > 40000) throw new Error("SOURCE_PROOF_INPUT_BOUND");
    let bytes;
    try { bytes = await sourceBytes(path, budget); }
    catch (error) {
      if (error.code !== "ENOENT") throw error;
      shared.push([name, null]);
      await absentInput(path, budget);
      continue;
    }
    if ((budget.bytes += bytes.length) > 512 * 1024 ** 2) throw new Error("SOURCE_PROOF_INPUT_BOUND");
    shared.push([name, createHash("sha256").update(bytes).digest("hex")]);
  }
  // Root-level include_str!/include_bytes! inputs (e.g. deploy examples) are real
  // compiler inputs even though Cargo has no package dependency edge for them.
  const sharedDigest = hash(shared);
  for (const item of locals.values()) {
    const seen = new Set();
    function dependencies(id) {
      if (seen.has(id)) return;
      if (!nodes.has(id)) throw new Error("SOURCE_PROOF_DEPENDENCIES_UNOBSERVED");
      seen.add(id);
      for (const next of nodes.get(id)) dependencies(next);
    }
    dependencies(item.id);
    item.provenance = hash({ sharedDigest, dependencies: [...seen].sort().map(id => locals.has(id)
      ? [hash(id), locals.get(id).identity, locals.get(id).content] : [hash(id)]) });
  }
  return { locals, names, digest: hash([...locals.values()].map(item => [item.identity, item.content, item.provenance]).sort()),
    stability: hash(budget.stability.sort(([a], [b]) => a.localeCompare(b))),
    counts: { packages: locals.size, filesAndDirectories: budget.entries, bytes: budget.bytes } };
}
