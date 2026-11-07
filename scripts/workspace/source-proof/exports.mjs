// Cargo may create primary library hardlinks even when its compiler unit is fresh.
// These invocation-local witnesses never relax persistent companion signatures.
import { lstat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, dirname, join, relative, resolve } from "node:path";
import { guarded } from "../paths.mjs";
import { boundedInput } from "../retention/policy.mjs";
import { hash } from "./inputs.mjs";
import { primaryProfile } from "./units.mjs";
const maximumFile = 32 * 1024 ** 2;
const maximumTotal = 64 * 1024 ** 2;
const same = (a, b) => hash(a) === hash(b);
const identity = info => [info.dev, info.ino, info.size, info.mtimeNs, info.ctimeNs].map(String);

async function bytes(lane, path) {
  await guarded(lane, path);
  const before = await lstat(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.size > BigInt(maximumFile)) throw new Error("SOURCE_PROOF_WITNESS_BOUND");
  const digest = createHash("sha256").update(await boundedInput(path, false, maximumFile)).digest("hex");
  const after = await lstat(path, { bigint: true });
  if (!same(identity(before), identity(after))) throw new Error("SOURCE_PROOF_ARTIFACT_CHANGED");
  return { identity: identity(after), digest };
}
function plausible(unit, request) {
  const args = request.args;
  const packages = [];
  let profile = args.includes("--release") || request.command === "bench" ? "release" : "debug";
  for (let i = 0; i < args.length && args[i] !== "--"; i++) {
    if (["-p", "--package"].includes(args[i])) packages.push(args[++i]);
    else if (args[i].startsWith("--package=")) packages.push(args[i].slice(10));
    else if (args[i] === "--profile") profile = args[++i];
    else if (args[i].startsWith("--profile=")) profile = args[i].slice(10);
  }
  if (profile === "dev" || profile === "test") profile = "debug";
  if (profile === "bench") profile = "release";
  // Cargo also accepts URLs/qualified package specs. Unknown selectors widen the
  // witness/rebuild candidate set rather than guessing that no unit is selected.
  const unknown = packages.some(value => !/^[A-Za-z0-9_-]+(?:@[A-Za-z0-9_.+-]+)?$/.test(value ?? ""));
  return unit.prefix === "lib-" && basename(unit.profile) === profile
    && (unknown || !packages.length || packages.some(value => value === unit.pkg.name || value?.startsWith(`${unit.pkg.name}@`)));
}

export async function prepareExportWitnesses(request, trusted) {
  const witnesses = new Map(), invalidated = new Set();
  let total = 0;
  for (const unit of [...trusted.values()].sort((a, b) => a.id.localeCompare(b.id))) {
    if (!plausible(unit, request)) continue;
    const records = [];
    const primary = primaryProfile(request.lane, unit.profile);
    for (const [path, prior] of unit.companions) {
      const name = basename(path);
      if (!path.includes("/deps/") || !name.includes(`-${unit.hash}.`) || !/\.(?:rlib|rmeta|a|lib)$/.test(name)) continue;
      const alias = join(primary, name.replace(`-${unit.hash}.`, "."));
      const aliasKey = relative(request.lane, alias).replaceAll("\\", "/");
      const existing = unit.companions.find(([known]) => known === aliasKey);
      if (existing) {
        if (!same(existing[1], prior)) invalidated.add(unit.id);
        continue;
      }
      if (Number(prior[2]) > maximumFile || total + Number(prior[2]) > maximumTotal) {
        invalidated.add(unit.id); continue;
      }
      total += Number(prior[2]);
      const witness = await bytes(request.lane, resolve(request.lane, path));
      if (!same(witness.identity, prior)) throw new Error("SOURCE_PROOF_ARTIFACT_CHANGED");
      records.push({ path, alias: aliasKey, ...witness });
    }
    if (records.length && !invalidated.has(unit.id)) witnesses.set(unit.id, records);
  }
  return { witnesses, invalidated };
}

export async function freshExportTransition(request, previous, unit, records) {
  if (!previous || !records?.length || previous.controlSignature !== unit.controlSignature
    || previous.pkg.provenance !== unit.pkg.provenance) return false;
  const before = new Map(previous.companions.map(value => [value[0], value]));
  const after = new Map(unit.companions.map(value => [value[0], value]));
  const allowed = new Set();
  let aliases = 0;
  for (const record of records) {
    const source = after.get(record.path), alias = after.get(record.alias);
    if (!source || !alias || !same(source[1].slice(0, 4), record.identity.slice(0, 4))
      || !same(source[1], alias[1])) continue;
    const observed = await bytes(request.lane, resolve(request.lane, record.path));
    if (!same(observed.identity, source[1]) || observed.digest !== record.digest) return false;
    allowed.add(record.path); allowed.add(record.alias); aliases++;
  }
  if (!aliases) return false;
  for (const [path, old] of before) {
    if (!after.has(path) || (!same(old, after.get(path)) && !allowed.has(path))) return false;
  }
  const primary = primaryProfile(request.lane, unit.profile);
  const names = new Set([unit.target, unit.target.replaceAll("-", "_")]);
  for (const [path, value] of after) {
    if (before.has(path) || allowed.has(path)) continue;
    // A new bounded derived primary dep export is meaningful only alongside a
    // witnessed new codegen hardlink, never as independent approval authority.
    if (resolve(dirname(resolve(request.lane, path))) !== resolve(primary)
      || ![...names].some(name => basename(path) === `${name}.d` || basename(path) === `lib${name}.d`)
      || value[1][0] !== "primary-dep-export" || !value[2]) return false;
  }
  return true;
}
