// Bounded selection evidence, never an admission rule or filesystem authority.
import { basename, extname, resolve } from "node:path";
import { hash } from "./inputs.mjs";

const name = value => typeof value === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(value) ? value : "redacted";
export function selectionFailure(message, units, selected, reconstructed, state, approved) {
  const local = [...units.values()].filter(unit => unit.pkg.id === message.package_id);
  const target = local.filter(unit => unit.target === message.target?.name);
  const kinds = Array.isArray(message.target?.kind) ? message.target.kind.slice(0, 8).map(name) : [];
  const files = Array.isArray(message.filenames) ? message.filenames : [];
  const extension = path => [".exe", ".rlib", ".rmeta", ".dll", ".lib", ".pdb", ".d", ".so", ".dylib"]
    .includes(extname(path)) ? extname(path) : "other";
  const detail = {
    schema: 1, reason: message.reason === "build-script-executed" ? "build-script-executed" : "compiler-artifact",
    package: name(local[0]?.pkg.name), target: name(message.target?.name), kinds,
    fresh: typeof message.fresh === "boolean" ? message.fresh : null,
    packageUnits: local.length, targetUnits: target.length,
    featureUnits: Array.isArray(message.features) ? target.filter(unit => hash(unit.features) === hash([...message.features].sort())).length : null,
    sourceUnits: typeof message.target?.src_path === "string" ? target.filter(unit => unit.pkg.targets.some(([targetName, path]) =>
      targetName === message.target.name && resolve(path) === resolve(message.target.src_path))).length : null,
    selected: selected.length, reconstructed: reconstructed.length,
    featureCount: Array.isArray(message.features) ? message.features.length : null,
    fileTypes: [...new Set(files.filter(path => typeof path === "string").map(extension))].slice(0, 10),
    candidates: (selected.length ? selected : target.length ? target : local).slice(0, 8).map(unit => ({
      unit: hash(unit.id).slice(0, 12), prefix: name(unit.prefix),
      profile: ["debug", "release"].includes(basename(unit.profile)) ? basename(unit.profile) : "custom",
      invalidated: state.invalidated.has(unit.id), newUnit: !state.units.has(unit.id),
      controlsChanged: state.units.has(unit.id) && state.units.get(unit.id).controlSignature !== unit.controlSignature,
      approved: !!approved[unit.id],
      knownFiles: files.filter(path => typeof path === "string" && unit.companions.some(([known]) =>
        resolve(state.request.lane, known) === resolve(path))).length,
    })),
  };
  const error = new Error(`SOURCE_PROOF_SELECTED_UNIT_UNOBSERVED ${JSON.stringify(detail)}`);
  error.sourceProofSelection = detail;
  return error;
}


// Compare only observations already sealed by inputSnapshot; never reread disk here.
export function inputFailure(before, after) {
  const group = snapshot => {
    const result = new Map();
    for (const row of snapshot.observations ?? []) {
      const key = `${row.id}:${row.kind}`;
      const values = result.get(key) ?? [];
      values.push(row.signature); result.set(key, values);
    }
    return result;
  };
  const old = group(before), current = group(after), changed = [];
  const fields = ["device", "inode", "size", "mtime", "ctime"];
  for (const key of [...new Set([...old.keys(), ...current.keys()])].sort()) {
    const left = old.get(key), right = current.get(key);
    if (hash(left ?? null) === hash(right ?? null)) continue;
    const separator = key.indexOf(":");
    const values = new Set();
    if (!left || !right) values.add("presence");
    else if (left.length !== right.length) values.add("observations");
    else for (let index = 0; index < left.length; index++) {
      if (!Array.isArray(left[index]) || !Array.isArray(right[index])) {
        if (left[index] !== right[index]) values.add("presence");
      } else for (let field = 0; field < fields.length; field++) {
        if (left[index][field] !== right[index][field]) values.add(fields[field]);
      }
    }
    const rawKind = key.slice(separator + 1);
    const knownKinds = ["package-file", "package-directory", "shared-file", "policy-file", "absent-input", "absent-input-ancestor",
      "policy-absent", "policy-absent-ancestor", "shared-absent", "shared-absent-ancestor"];
    changed.push({ witness: hash(key.slice(0, separator)).slice(0, 12),
      kind: knownKinds.includes(rawKind) ? rawKind : "other", fields: [...values] });
  }
  const packages = [];
  for (const id of new Set([...before.locals.keys(), ...after.locals.keys()])) {
    const left = before.locals.get(id), right = after.locals.get(id);
    if (left?.identity !== right?.identity || left?.content !== right?.content || left?.provenance !== right?.provenance) {
      packages.push({ name: name(right?.name ?? left?.name), presence: !!left !== !!right,
        content: left?.content !== right?.content, identity: left?.identity !== right?.identity,
        provenance: left?.provenance !== right?.provenance });
    }
  }
  const detail = { schema: 1, digestChanged: before.digest !== after.digest, stabilityChanged: before.stability !== after.stability,
    changedObservations: changed.length, observations: changed.slice(0, 8),
    changedPackages: packages.length, packages: packages.slice(0, 8) };
  const error = new Error(`SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND ${JSON.stringify(detail)}`);
  error.sourceProofInputs = detail;
  return error;
}
