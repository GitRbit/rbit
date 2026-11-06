// One command's opaque proof. Disk records cannot select mutation paths or approve outputs.
import { resolve } from "node:path";
import { requireLease } from "../storage.mjs";
import { cargoTreeMode, verifiedCargoCompletion } from "../cargo-tree.mjs";
import { inputSnapshot, hash } from "./inputs.mjs";
import { scanUnits, invalidateStamp } from "./units.mjs";
import { prepareExportWitnesses, freshExportTransition } from "./exports.mjs";
import { ledger, saveLedger } from "./ledger.mjs";
import { guarded, managedDirectory } from "../paths.mjs";
import { UncertainProcessTreeError } from "../lease.mjs";
const commands = new WeakMap();
const same = (unit, entry) => entry?.provenance === unit.pkg.provenance && entry?.signature === unit.signature;

export async function beginSourceProof(ctx, request, lease, dependencies = {}) {
  await requireLease(ctx, lease);
  // Native control setup must precede absence/ancestor observations, otherwise its
  // first state-directory creation could look like a source/config ABA mutation.
  await managedDirectory(ctx, ctx.state);
  const { lane, namespace, command, args, env } = request;
  const inputs = await inputSnapshot(ctx, namespace, command, args, env, dependencies);
  const units = await scanUnits(lane, inputs);
  const state = await ledger(ctx, lane, lease);
  const trusted = new Map(), invalidated = new Set();
  for (const unit of units.values()) {
    if (same(unit, state.entries[unit.id])) trusted.set(unit.id, unit);
    else invalidated.add(unit.id);
  }
  const exports = await prepareExportWitnesses(request, trusted);
  for (const id of exports.invalidated) { trusted.delete(id); invalidated.add(id); }
  // An interruption after any mutation cannot leave a completed provenance claim.
  await saveLedger(state, "pending", {}, lease);
  for (const id of invalidated) await invalidateStamp(ctx, lane, units.get(id), lease);
  const handle = Object.freeze({}), invocation = Object.freeze({});
  commands.set(handle, { ctx: Object.freeze({ ...ctx }), request: { ...request, args: [...args], env: { ...env } },
    token: lease.token, inputs, units, state, trusted, invalidated, invocation, dependencies, exports: exports.witnesses, messages: [] });
  return { handle, invocation, observe: message => {
    const value = commands.get(handle);
    if (!value || !["compiler-artifact", "build-script-executed"].includes(message?.reason) || !inputs.locals.has(message.package_id)) return;
    if (value.messages.length >= 20000) throw new Error("SOURCE_PROOF_MESSAGE_BOUND");
    value.messages.push(message);
  }, report: { invalidated: invalidated.size, reusable: trusted.size } };
}

function selected(message, units, lane) {
  let matches = [...units.values()].filter(unit => unit.pkg.id === message.package_id);
  if (message.reason === "build-script-executed") {
    if (typeof message.out_dir !== "string") throw new Error("SOURCE_PROOF_MESSAGE_REFUSED");
    return matches.filter(unit => unit.prefix === "run-build-script-"
      && resolve(message.out_dir) === resolve(unit.profile, "build", `${unit.pkg.name}-${unit.hash}`, "out"));
  }
  if (!message.target || !Array.isArray(message.filenames) || message.filenames.length > 256 || !Array.isArray(message.features)
    || typeof message.fresh !== "boolean") throw new Error("SOURCE_PROOF_MESSAGE_REFUSED");
  matches = matches.filter(unit => unit.prefix !== "run-build-script-" && unit.target === message.target.name
    && hash(unit.features) === hash([...message.features].sort())
    && unit.pkg.targets.some(([name, path]) => name === message.target.name && resolve(path) === resolve(message.target.src_path)));
  matches = matches.filter(unit => message.filenames.every(path => typeof path === "string" && unit.companions.some(([known]) =>
    resolve(lane, known) === resolve(path))));
  const hashed = matches.filter(unit => message.filenames.some(path => typeof path === "string" && path.split(/[\\/]/).some(part =>
    part === `${unit.pkg.name}-${unit.hash}` || new RegExp(`-${unit.hash}(?:\\.|$)`).test(part))));
  return hashed.length ? hashed : matches;
}

export async function finishSourceProof(handle, completion, lease) {
  const value = commands.get(handle);
  if (!value || value.token !== lease?.token) throw new Error("SOURCE_PROOF_AUTHORITY_UNPROVED");
  if (!verifiedCargoCompletion(completion, lease, value.invocation)
    && (completion !== undefined || cargoTreeMode(value.request.env) === "windows-job")) {
    const uncertain = new UncertainProcessTreeError("Shared Cargo completion is unproved for this invocation");
    try { await failSourceProof(handle, lease); }
    catch (diagnostic) { try { uncertain.provenanceRecordError = diagnostic; } catch {} }
    throw uncertain;
  }
  if (!verifiedCargoCompletion(completion, lease, value.invocation)) {
    // An explicitly unowned command cannot approve output. Record that refusal
    // without rereading source/configuration that may still be changing.
    await failSourceProof(handle, lease);
    return { trusted: false, reason: "OWNED_NATIVE_COMPLETION_REQUIRED", approved: 0 };
  }
  const { ctx, request, inputs, state, trusted } = value;
  const after = await inputSnapshot(ctx, request.namespace, request.command, request.args, request.env, value.dependencies, "completion");
  if (inputs.digest !== after.digest || inputs.stability !== after.stability) throw new Error("SOURCE_PROOF_INPUT_CHANGED_DURING_COMMAND");
  const units = await scanUnits(request.lane, after);
  const approved = {};
  for (const [id, previous] of trusted) {
    const unit = units.get(id);
    if (unit && previous.signature === unit.signature && previous.pkg.provenance === unit.pkg.provenance) {
      approved[id] = { provenance: unit.pkg.provenance, signature: unit.signature };
    }
  }
  for (const message of [...value.messages.filter(item => item.reason === "compiler-artifact"),
    ...value.messages.filter(item => item.reason === "build-script-executed")]) {
    for (const path of message.filenames ?? []) {
      if (typeof path !== "string") throw new Error("SOURCE_PROOF_MESSAGE_REFUSED");
      await guarded(request.lane, path);
    }
    let candidates = selected(message, units, request.lane);
    if (message.reason === "build-script-executed") {
      candidates = candidates.filter(runtime => [...units.values()].some(compiled => compiled.pkg.id === runtime.pkg.id
        && compiled.prefix === "build-script-" && compiled.fingerprint === runtime.scriptDependency && approved[compiled.id]));
    }
    if (message.reason === "build-script-executed" && candidates.length === 1 && approved[candidates[0].id]) continue;
    if (message.reason === "compiler-artifact" && message.fresh) {
      for (const unit of candidates) {
        if (!approved[unit.id] && await freshExportTransition(request, trusted.get(unit.id), unit, value.exports.get(unit.id))) {
          approved[unit.id] = { provenance: unit.pkg.provenance, signature: unit.signature };
        }
      }
      if (!candidates.length || candidates.some(unit => !approved[unit.id])) throw new Error("SOURCE_PROOF_UNEXPECTED_FRESH_UNIT");
      continue;
    }
    // Unknown stamps were removed before invocation. Only observed recreation or
    // changed compiler controls can authorize a new proof; presence alone cannot.
    candidates = candidates.filter(unit => value.invalidated.has(unit.id) || !value.units.has(unit.id)
      || value.units.get(unit.id).controlSignature !== unit.controlSignature);
    if (candidates.length !== 1) throw new Error("SOURCE_PROOF_SELECTED_UNIT_UNOBSERVED");
    const unit = candidates[0];
    approved[unit.id] = { provenance: unit.pkg.provenance, signature: unit.signature };
  }
  await requireLease(ctx, lease);
  await saveLedger(state, "complete", approved, lease);
  commands.delete(handle);
  return { trusted: true, approved: Object.keys(approved).length, invalidated: value.invalidated.size, reusable: trusted.size,
    fresh: value.messages.filter(message => message.reason === "compiler-artifact" && message.fresh === true).length,
    rebuilt: value.messages.filter(message => message.reason === "compiler-artifact" && message.fresh === false).length };
}

export async function failSourceProof(handle, lease) {
  const value = commands.get(handle);
  if (!value || value.token !== lease?.token) return;
  commands.delete(handle);
  // Do not read potentially active source/artifact trees on an uncertain failure.
  await saveLedger(value.state, "untrusted", {}, lease);
}

export const sourceProof = Object.freeze({ begin: beginSourceProof, finish: finishSourceProof, fail: failSourceProof });
