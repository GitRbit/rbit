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
