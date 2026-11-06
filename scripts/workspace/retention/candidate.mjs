// Explicit fresh-output measurement only. One candidate, no promotion or artifact removal.
import { randomUUID } from "node:crypto";
import { mkdir, opendir } from "node:fs/promises";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { guarded, managedDirectory } from "../paths.mjs";
import { requireLease } from "../storage.mjs";
import { UncertainProcessTreeError } from "../lease.mjs";
import { verifiedCargoCompletion, failedCargoCompletion } from "../cargo-tree.mjs";
import { policySnapshot } from "./policy.mjs";
import { freeBytes, namespaceUsage } from "./usage.mjs";
import { proveHandoff } from "./handoff.mjs";
import { writeGenerationRecord } from "./generation-record.mjs";
const candidates = new WeakMap();

export function candidateHeadroom(env) {
  const value = env.RBIT_BUILD_CANDIDATE_HEADROOM_BYTES;
  if (typeof value !== "string" || !/^[1-9]\d*$/.test(value) || !Number.isSafeInteger(Number(value))) {
    throw new Error("RBIT_BUILD_CANDIDATE_HEADROOM_BYTES must explicitly state positive required free bytes");
  }
  return Number(value);
}
export function requireCandidateHeadroom(available, required) {
  if (!Number.isSafeInteger(available) || available < 0 || !Number.isSafeInteger(required)
    || required <= 0 || available < required) throw new Error("CANDIDATE_HEADROOM_UNPROVED");
}
export async function createCandidate(ctx, namespace, requiredFreeBytes, lease) {
  await requireLease(ctx, lease);
  if (!/^[a-f0-9]{20}$/.test(namespace) || !Number.isSafeInteger(requiredFreeBytes) || requiredFreeBytes <= 0) throw new Error("CANDIDATE_INPUT_REFUSED");
  const available = await freeBytes(ctx.cache);
  requireCandidateHeadroom(available, requiredFreeBytes);
  const root = await managedDirectory(ctx, join(ctx.cache, namespace, "managed"));
  // Unknown/orphan/quarantined/successful entries all refuse another candidate.
  // Disk records never authorize reuse, promotion or recursive cleanup here.
  for await (const _entry of await opendir(root)) throw new Error("CANDIDATE_ALREADY_PRESENT_OR_UNRECOGNIZED");
  const id = randomUUID();
  const path = await guarded(ctx.root, join(root, `candidate-${id}`));
  const record = { schema: 1, source: "managed-candidate", namespace, id,
    kind: "measurement-candidate", status: "allocated", eligibleForPromotion: false,
    eligibleForEviction: false, headroom: { requiredFreeBytes, observedFreeBytes: available, reserved: false } };
  const pointer = join(root, "candidate.json");
  const file = join(path, "generation.json");
  // Reserve the singleton with wx before creating its directory, including against
  // accidental concurrent callers within the same already-held management lease.
  await writeGenerationRecord(ctx, pointer, record, lease, true);
  await mkdir(path);
  await writeGenerationRecord(ctx, file, record, lease, true);
  const handle = Object.freeze({});
  candidates.set(handle, { ctx: Object.freeze({ ...ctx }), id, namespace, path, file, pointer, record, token: lease.token });
  return { handle, path };
}
function owned(handle, lease) {
  const value = handle && typeof handle === "object" ? candidates.get(handle) : undefined;
  if (!value || typeof lease?.token !== "string" || !lease.token || value.token !== lease.token) throw new Error("CANDIDATE_AUTHORITY_UNPROVED");
  return value;
}
async function publish(value, record, lease) {
  await writeGenerationRecord(value.ctx, value.file, record, lease);
  await writeGenerationRecord(value.ctx, value.pointer, record, lease);
  value.record = record;
}
export async function startCandidate(handle, command, args, env, lease, { read } = {}) {
  const value = owned(handle, lease);
  if (value.request || value.record.status !== "allocated") throw new Error("CANDIDATE_EXECUTION_STATE_REFUSED");
  await requireLease(value.ctx, lease);
  const before = await policySnapshot(value.ctx, value.namespace, command, args, env, read);
  value.request = { command, args: [...args], env: { ...env }, read, before };
  await publish(value, { ...value.record, status: "measuring", graph: before.graph }, lease);
}
/** Called synchronously immediately before handing control to the executor. */
export function candidateExecutionStarting(handle, lease) {
  const value = owned(handle, lease);
  if (!value.request || value.record.status !== "measuring" || value.executionStarted) throw new Error("CANDIDATE_EXECUTION_STATE_REFUSED");
  value.executionStarted = true;
  value.started = performance.now();
}
export async function completeCandidate(handle, completion, copies, lease) {
  const value = owned(handle, lease);
  if (!verifiedCargoCompletion(completion, lease) || !value.request || value.commandWallMs === undefined) throw new Error("CANDIDATE_COMPLETION_UNPROVED");
  await requireLease(value.ctx, lease);
  const { command, args, env, read, before } = value.request;
  const after = await policySnapshot(value.ctx, value.namespace, command, args, env, read);
  const graph = before.inputId && before.inputId === after.inputId ? after.graph
    : { confidence: "conservative", reasons: ["POLICY_INPUTS_CHANGED_OR_UNOBSERVED"] };
  const handoff = await proveHandoff(value.ctx, value.path, copies);
  const usage = await namespaceUsage(value.path);
  const record = { ...value.record, status: "succeeded", graph,
    inputId: graph.confidence === "known" ? after.inputId : null,
    completion: { method: completion.method, treeStopped: true }, handoff, usage,
    measurement: { commandWallMs: value.commandWallMs, freshOutputDirectories: true,
      nativeLauncherIncluded: true, peakMemoryMeasured: false, commandCompleted: true,
      compilerCacheState: "unproved", osPageCacheState: "unmeasured", coldCostGateEligible: false,
      wrapperEnvironmentDetected: Object.entries(env).some(([key, value]) =>
        /^(RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_BUILD_RUSTC_WRAPPER|CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER)$/i.test(key)
          && typeof value === "string" && value.length > 0),
      automaticRotationApproved: false } };
  await publish(value, record, lease);
  candidates.delete(handle);
  return { mode: "measure-candidate", id: value.id, status: "succeeded", graph, usage,
    measurement: record.measurement, eligibleForPromotion: false, eligibleForEviction: false };
}
export function candidateExecuted(handle, lease) {
  const value = owned(handle, lease);
  if (value.started === undefined || value.commandWallMs !== undefined) throw new Error("CANDIDATE_EXECUTION_STATE_REFUSED");
  value.commandWallMs = Math.max(0, performance.now() - value.started);
}
/** Failure records use only fresh native receipt identity, never an error JSON field. */
export async function quarantineCandidate(handle, error, successCompletion, lease) {
  const value = owned(handle, lease);
  await requireLease(value.ctx, lease);
  const proof = verifiedCargoCompletion(successCompletion, lease) ? successCompletion : failedCargoCompletion(error, lease);
  const uncertain = error instanceof UncertainProcessTreeError || (value.executionStarted && !proof);
  const record = { ...value.record, status: uncertain ? "uncertain" : "failed",
    failure: { code: uncertain ? "EXECUTION_OR_COMPLETION_UNPROVED"
      : value.executionStarted ? "COMPLETED_COMMAND_OR_PUBLICATION_FAILED" : "PRE_EXECUTION_FAILED",
      nativeTreeStopped: !!proof, commandStarted: value.executionStarted === true }, eligibleForPromotion: false, eligibleForEviction: false };
  // Do not scan or hash a potentially still-changing unproved process tree.
  await publish(value, record, lease);
  candidates.delete(handle);
}
